"use client";

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Copy, Download, FolderOpen, RefreshCw, Users } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { IdentifySpeakersDialog } from '@/components/Speakers/IdentifySpeakersDialog';
import { useConfig } from '@/contexts/ConfigContext';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
  onIdentifySpeakers?: (numSpeakers: number | null) => Promise<void>;
  hasSpeakers?: boolean;
  speakerJobActive?: boolean;
  isRecording?: boolean;
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
  onIdentifySpeakers,
  hasSpeakers,
  speakerJobActive,
  isRecording = false,
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);
  const [showIdentifyDialog, setShowIdentifyDialog] = useState(false);
  const [exporting, setExporting] = useState(false);

  const exportTranscript = useCallback(async (format: 'txt' | 'vtt') => {
    if (!meetingId || exporting) {
      return;
    }
    setExporting(true);
    Analytics.trackButtonClick(`export_transcript_${format}`, 'meeting_details');
    try {
      const result = await invoke<{
        path: string;
        segments_written: number;
        segments_skipped: number;
      } | null>('export_meeting_transcript', { meeting_id: meetingId, format });
      if (!result) {
        return;
      }
      if (result.segments_skipped > 0) {
        toast.warning('Transcript exported', {
          description: `${result.segments_written} lines saved. ${result.segments_skipped} had no timing and were left out of the captions.`,
        });
      } else {
        toast.success('Transcript exported');
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error('Could not export the transcript', { description: message });
    } finally {
      setExporting(false);
    }
  }, [meetingId, exporting]);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          className="px-2 @[22rem]:px-3"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden @[22rem]:inline">Copy</span>
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              className="px-2 @[22rem]:px-3"
              disabled={transcriptCount === 0 || isRecording || exporting || !meetingId}
              title={
                transcriptCount === 0
                  ? 'No transcript available'
                  : isRecording
                    ? 'Export is available after recording stops'
                    : 'Export transcript'
              }
            >
              <Download />
              <span className="hidden @[22rem]:inline">Export</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onClick={() => void exportTranscript('txt')}>
              Plain text (.txt)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void exportTranscript('vtt')}>
              WebVTT (.vtt)
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <Button
          size="sm"
          variant="outline"
          className="px-2 @[22rem]:px-4"
          onClick={() => {
            Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
            onOpenMeetingFolder();
          }}
          title="Open Recording Folder"
        >
          <FolderOpen className="@[22rem]:mr-2" size={18} />
          <span className="hidden @[22rem]:inline">Recording</span>
        </Button>

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title="Retranscribe to enhance your recorded audio"
          >
            <RefreshCw className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Enhance</span>
          </Button>
        )}
        {betaFeatures.speakerIdentification && onIdentifySpeakers && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="px-2 @[22rem]:px-4"
            disabled={speakerJobActive}
            onClick={() => {
              Analytics.trackButtonClick('identify_speakers', 'meeting_details');
              setShowIdentifyDialog(true);
            }}
            title="Identify who is speaking"
          >
            <Users className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Speakers</span>
          </Button>
        )}
      </ButtonGroup>

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}

      {betaFeatures.speakerIdentification && onIdentifySpeakers && (
        <IdentifySpeakersDialog
          open={showIdentifyDialog}
          onOpenChange={setShowIdentifyDialog}
          hasSpeakers={!!hasSpeakers}
          onRun={onIdentifySpeakers}
        />
      )}
    </div>
  );
}
