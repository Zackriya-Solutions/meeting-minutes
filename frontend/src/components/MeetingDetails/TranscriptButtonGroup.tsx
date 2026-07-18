"use client";

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Loader2, Pause, Play, RefreshCw } from '@/components/memento/LucideCompat';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { DetectSpeakersButton } from './DetectSpeakersButton';
import { SpeakerNameCandidatesButton } from './SpeakerNameCandidatesButton';
import { DeleteMeetingButton } from './DeleteMeetingButton';
import { useConfig } from '@/contexts/ConfigContext';
import { useT } from '@/lib/i18n';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onToggleRecordingAudio: () => void;
  recordingAudioAvailable: boolean;
  isRecordingAudioPlaying: boolean;
  isRecordingAudioLoading: boolean;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
  /** Refresh speakers + transcripts after a successful speaker detection. */
  onSpeakersDetected?: () => Promise<void> | void;
  speakerCount?: number;
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onToggleRecordingAudio,
  recordingAudioAvailable,
  isRecordingAudioPlaying,
  isRecordingAudioLoading,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
  onSpeakersDetected,
  speakerCount = 0,
}: TranscriptButtonGroupProps) {
  const t = useT();
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex w-full items-center justify-start gap-2 overflow-x-auto xl:justify-center">
      <ButtonGroup className="shrink-0">
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? t('No transcript available') : t('Copy Transcript')}
        >
          <Copy />
          <span className="hidden lg:inline">{t('Copy')}</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="xl:px-4"
          onClick={() => {
            Analytics.trackButtonClick('toggle_recording_playback', 'meeting_details');
            onToggleRecordingAudio();
          }}
          disabled={!recordingAudioAvailable || isRecordingAudioLoading}
          title={isRecordingAudioPlaying ? t('Pause meeting audio') : t('Play meeting audio')}
        >
          {isRecordingAudioLoading
            ? <Loader2 className="animate-spin xl:mr-2" size={18} />
            : isRecordingAudioPlaying
              ? <Pause className="xl:mr-2" size={18} />
              : <Play className="xl:mr-2" size={18} />}
          <span className="hidden lg:inline">{t('Recording')}</span>
        </Button>

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="border-[var(--gold-border)] bg-[var(--gold)] hover:bg-[var(--gold-active)] xl:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title={t('Retranscribe to enhance your recorded audio')}
          >
            <RefreshCw className="xl:mr-2" size={18} />
            <span className="hidden lg:inline">{t('Enhance')}</span>
          </Button>
        )}

        <DetectSpeakersButton meetingId={meetingId} speakerCount={speakerCount} onDetected={onSpeakersDetected} />
        <SpeakerNameCandidatesButton meetingId={meetingId} onApplied={onSpeakersDetected} />
        <DeleteMeetingButton
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
        />
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
    </div>
  );
}
