/**
 * useTranscriptRecovery Hook
 *
 * Orchestrates transcript recovery operations for interrupted meetings.
 * Provides functionality to detect, preview, and recover meetings from IndexedDB.
 */

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { indexedDBService, MeetingMetadata, StoredTranscript } from '@/services/indexedDBService';
import { storageService } from '@/services/storageService';
import { applyPinnedSummaryLanguageToMeeting } from '@/lib/summary-language-preferences';
import { toast } from 'sonner';

interface AudioRecoveryStatus {
  status: string; // "success" | "partial" | "failed" | "none"
  chunk_count: number;
  estimated_duration_seconds: number;
  audio_file_path?: string;
  message: string;
}

export interface UseTranscriptRecoveryReturn {
  recoverableMeetings: MeetingMetadata[];
  isLoading: boolean;
  isRecovering: boolean;
  checkForRecoverableTranscripts: () => Promise<void>;
  recoverMeeting: (meetingId: string) => Promise<{ success: boolean; audioRecoveryStatus?: AudioRecoveryStatus | null; meetingId?: string }>;
  loadMeetingTranscripts: (meetingId: string) => Promise<StoredTranscript[]>;
  deleteRecoverableMeeting: (meetingId: string) => Promise<void>;
}

export function useTranscriptRecovery(): UseTranscriptRecoveryReturn {
  const [recoverableMeetings, setRecoverableMeetings] = useState<MeetingMetadata[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isRecovering, setIsRecovering] = useState(false);

  /**
   * Check for recoverable meetings in IndexedDB
   */
  const checkForRecoverableTranscripts = useCallback(async () => {
    setIsLoading(true);
    try {
      const meetings = await indexedDBService.getAllMeetings();

      // Filter out meetings older than 7 days and newer than 15 seconds
      // The 15 seconds threshold prevents showing meetings from the current session(jus in case)
      // where recording just stopped but hasn't been fully saved yet
      const cutoffTime = Date.now() - (7 * 24 * 60 * 60 * 1000);
      const secondsAgo = Date.now() - (2 * 1000);

      const recentMeetings = meetings.filter(m => {
        const isWithinRetention = m.lastUpdated > cutoffTime; // Not older than 7 days
        const isOldEnough = m.lastUpdated < secondsAgo; // Older than 15 seconds
        return isWithinRetention && isOldEnough;
      });

      // Verify audio checkpoint availability for each meeting
      const meetingsWithAudioStatus = await Promise.all(
        recentMeetings.map(async (meeting) => {
          if (meeting.folderPath) {
            try {
              const hasAudio = await invoke<boolean>('has_audio_checkpoints', {
                meetingFolder: meeting.folderPath
              });

              // If no audio files, clear folderPath to show "No audio" in UI
              return {
                ...meeting,
                folderPath: hasAudio ? meeting.folderPath : undefined
              };
            } catch (error) {
              console.warn('Failed to check audio for meeting:', error);
              // On error, assume no audio to be safe
              return { ...meeting, folderPath: undefined };
            }
          }
          return meeting;
        })
      );


      setRecoverableMeetings(meetingsWithAudioStatus);
    } catch (error) {
      console.error('Failed to check for recoverable transcripts:', error);
      setRecoverableMeetings([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Load transcripts for preview
   */
  const loadMeetingTranscripts = useCallback(async (meetingId: string): Promise<StoredTranscript[]> => {
    try {
      const transcripts = await indexedDBService.getTranscripts(meetingId);
      // Sort by sequence ID
      transcripts.sort((a, b) => (a.sequenceId || 0) - (b.sequenceId || 0));
      return transcripts;
    } catch (error) {
      console.error('Failed to load meeting transcripts:', error);
      return [];
    }
  }, []);

  /**
   * Recover a meeting from IndexedDB
   */
  const recoverMeeting = useCallback(async (meetingId: string): Promise<{ success: boolean; audioRecoveryStatus?: AudioRecoveryStatus | null; meetingId?: string }> => {
    setIsRecovering(true);
    try {
      // 1. Load meeting metadata
      const metadata = await indexedDBService.getMeetingMetadata(meetingId);
      if (!metadata) {
        throw new Error('Meeting metadata not found');
      }

      // 2. Load all transcripts
      const transcripts = await loadMeetingTranscripts(meetingId);
      if (transcripts.length === 0) {
        throw new Error('No transcripts found for this meeting');
      }

      // 3. Check for folder path
      let folderPath = metadata.folderPath;


      if (!folderPath) {
        // Try to get from backend (might exist if only app crashed, not system)
        try {
          folderPath = await invoke<string>('get_meeting_folder_path');
        } catch (error) {
          folderPath = undefined;
        }
      }

      // 4. Attempt audio recovery if folder path exists
      let audioRecoveryStatus: AudioRecoveryStatus | null = null;
      if (folderPath) {
        try {
          audioRecoveryStatus = await invoke<AudioRecoveryStatus>(
            'recover_audio_from_checkpoints',
            { meetingFolder: folderPath, sampleRate: 48000 }
          );
        } catch (error) {
          console.error('Audio recovery failed:', error);
          audioRecoveryStatus = {
            status: 'failed',
            chunk_count: 0,
            estimated_duration_seconds: 0,
            message: error instanceof Error ? error.message : 'Unknown error'
          };
        }
      } else {
        audioRecoveryStatus = {
          status: 'none',
          chunk_count: 0,
          estimated_duration_seconds: 0,
          message: 'No folder path available'
        };
      }

      // 5. Convert StoredTranscripts to the format expected by storageService
      const formattedTranscripts = transcripts.map((t, index) => ({
        id: t.id?.toString() || `${Date.now()}-${index}`,
        text: t.text,
        timestamp: t.timestamp,
        sequence_id: t.sequenceId || index,
        chunk_start_time: (t as any).chunk_start_time,
        is_partial: (t as any).is_partial || false,
        confidence: t.confidence,
        audio_start_time: (t as any).audio_start_time,
        audio_end_time: (t as any).audio_end_time,
        duration: (t as any).duration,
        attribution_source: (t as any).attribution_source,
        overlap_region_id: (t as any).overlap_region_id,
        overlap_speaker_ids: (t as any).overlap_speaker_ids,
        overlap_start_time: (t as any).overlap_start_time,
        overlap_end_time: (t as any).overlap_end_time,
        overlap_confidence: (t as any).overlap_confidence,
        overlap_status: (t as any).overlap_status,
      }));

      // 6. Save to backend database using existing save utilities
      const saveResponse = await storageService.saveMeeting(
        metadata.title,
        formattedTranscripts,
        folderPath ?? null
      );

      const savedMeetingId = saveResponse.meeting_id;

      try {
        await applyPinnedSummaryLanguageToMeeting(savedMeetingId);
      } catch (error) {
        console.warn('Failed to apply pinned summary language to recovered meeting:', error);
        toast.warning('Could not apply default summary language', {
          description: 'The recovered meeting was saved, but the default summary language was not applied.',
        });
      }

      // 7. Mark as saved in IndexedDB
      await indexedDBService.markMeetingSaved(meetingId);


      // 8. Clean up checkpoint files
      if (folderPath) {
        try {
          await invoke('cleanup_checkpoints', { meetingFolder: folderPath });
        } catch (error) {
          // Non-fatal - don't fail recovery if cleanup fails
          console.warn('Checkpoint cleanup failed (non-fatal):', error);
        }
      }

      // 9. Remove from recoverable list
      setRecoverableMeetings(prev => prev.filter(m => m.meetingId !== meetingId));

      return {
        success: true,
        audioRecoveryStatus,
        meetingId: savedMeetingId
      };
    } catch (error) {
      console.error('Failed to recover meeting:', error);
      throw error;
    } finally {
      setIsRecovering(false);
    }
  }, [loadMeetingTranscripts]);

  /**
   * Delete a recoverable meeting
   */
  const deleteRecoverableMeeting = useCallback(async (meetingId: string): Promise<void> => {
    try {
      await indexedDBService.deleteMeeting(meetingId);
      setRecoverableMeetings(prev => prev.filter(m => m.meetingId !== meetingId));
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      throw error;
    }
  }, []);

  return {
    recoverableMeetings,
    isLoading,
    isRecovering,
    checkForRecoverableTranscripts,
    recoverMeeting,
    loadMeetingTranscripts,
    deleteRecoverableMeeting
  };
}
