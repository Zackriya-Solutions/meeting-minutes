"use client";

import { MeetingSpeaker, Transcript, TranscriptSegmentData } from '@/types';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptPanelProps) {
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker_id: t.speaker_id,
      speaker_label: t.speaker_label,
      speaker_color: t.speaker_color,
      is_overlap: t.is_overlap,
      diarization_status: t.diarization_status,
      diarization_method: t.diarization_method,
      diarization_confidence: t.diarization_confidence,
    }));
  }, [transcripts, usePagination, segments]);

  const [speakers, setSpeakers] = useState<MeetingSpeaker[]>([]);
  const [isLoadingSpeakers, setIsLoadingSpeakers] = useState(false);
  const [editingSpeakerId, setEditingSpeakerId] = useState<string | null>(null);
  const [draftSpeakerLabel, setDraftSpeakerLabel] = useState('');
  const [savingSpeakerId, setSavingSpeakerId] = useState<string | null>(null);

  const speakerSourceCount = usePagination
    ? totalCount ?? convertedSegments.length
    : transcripts.length;

  const loadSpeakers = useCallback(async () => {
    if (!meetingId || speakerSourceCount === 0) {
      setSpeakers([]);
      return;
    }

    setIsLoadingSpeakers(true);
    try {
      const loadedSpeakers = await invoke<MeetingSpeaker[]>('get_meeting_speakers', { meetingId });
      setSpeakers(loadedSpeakers);
    } catch (error) {
      console.error('Failed to load meeting speakers:', error);
      toast.error('Failed to load speakers');
    } finally {
      setIsLoadingSpeakers(false);
    }
  }, [meetingId, speakerSourceCount]);

  useEffect(() => {
    if (isRecording) {
      setSpeakers([]);
      setEditingSpeakerId(null);
      return;
    }

    void loadSpeakers();
  }, [isRecording, loadSpeakers]);

  const startEditingSpeaker = useCallback((speaker: MeetingSpeaker) => {
    setEditingSpeakerId(speaker.speakerId);
    setDraftSpeakerLabel(speaker.speakerLabel);
  }, []);

  const cancelEditingSpeaker = useCallback(() => {
    setEditingSpeakerId(null);
    setDraftSpeakerLabel('');
  }, []);

  const saveSpeakerLabel = useCallback(async (speaker: MeetingSpeaker) => {
    if (!meetingId) {
      return;
    }

    const label = draftSpeakerLabel.trim();
    if (!label) {
      toast.error('Speaker name cannot be empty');
      return;
    }

    if (label === speaker.speakerLabel) {
      cancelEditingSpeaker();
      return;
    }

    setSavingSpeakerId(speaker.speakerId);
    try {
      await invoke('rename_speaker', {
        request: {
          meetingId,
          speakerId: speaker.speakerId,
          label,
        },
      });

      setSpeakers(previousSpeakers =>
        previousSpeakers.map(previousSpeaker =>
          previousSpeaker.speakerId === speaker.speakerId
            ? { ...previousSpeaker, speakerLabel: label }
            : previousSpeaker
        )
      );
      cancelEditingSpeaker();

      if (onRefetchTranscripts) {
        await onRefetchTranscripts();
      }
      await loadSpeakers();

      toast.success('Speaker renamed');
    } catch (error) {
      console.error('Failed to rename speaker:', error);
      toast.error('Failed to rename speaker', {
        description: String(error),
      });
    } finally {
      setSavingSpeakerId(null);
    }
  }, [cancelEditingSpeaker, draftSpeakerLabel, loadSpeakers, meetingId, onRefetchTranscripts]);

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {!isRecording && meetingId && (speakers.length > 0 || isLoadingSpeakers) && (
        <div className="px-4 py-3 border-b border-border space-y-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Speakers{speakers.length > 0 ? ` (${speakers.length})` : ''}
              </p>
              <p className="text-xs text-muted-foreground">
                Names are applied to transcript labels and copied text.
              </p>
            </div>
            {isLoadingSpeakers && (
              <span className="text-xs text-muted-foreground">Loading…</span>
            )}
          </div>

          <div className="max-h-56 space-y-2 overflow-y-auto pr-1">
            {speakers.map((speaker) => {
              const isEditing = editingSpeakerId === speaker.speakerId;
              const isSaving = savingSpeakerId === speaker.speakerId;
              const transcriptSegmentLabel =
                speaker.segmentCount === 1 ? 'transcript segment' : 'transcript segments';

              return (
                <div
                  key={speaker.speakerId}
                  className="rounded-md border border-border bg-background/60 px-3 py-2"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className="h-2.5 w-2.5 rounded-full border border-border shrink-0"
                      style={{ backgroundColor: speaker.speakerColor || '#94a3b8' }}
                    />

                    {isEditing ? (
                      <>
                        <Input
                          aria-label="Speaker name"
                          className="h-8 min-w-0 flex-1"
                          value={draftSpeakerLabel}
                          disabled={isSaving}
                          onChange={(event) => setDraftSpeakerLabel(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter') {
                              void saveSpeakerLabel(speaker);
                            }
                            if (event.key === 'Escape') {
                              cancelEditingSpeaker();
                            }
                          }}
                        />
                        <Button
                          size="sm"
                          onClick={() => void saveSpeakerLabel(speaker)}
                          disabled={isSaving}
                        >
                          Save
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={cancelEditingSpeaker}
                          disabled={isSaving}
                        >
                          Cancel
                        </Button>
                      </>
                    ) : (
                      <>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-medium text-foreground">
                            {speaker.speakerLabel}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {speaker.segmentCount} {transcriptSegmentLabel}
                          </p>
                        </div>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => startEditingSpeaker(speaker)}
                          disabled={savingSpeakerId !== null}
                        >
                          Rename
                        </Button>
                      </>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
