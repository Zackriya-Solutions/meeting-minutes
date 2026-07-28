import { useConfig } from '@/contexts/ConfigContext';

export interface TranscriptEmptyState {
  title: string;
  subtitle: string;
  /** Tailwind classes for the status dot above the copy. */
  dotClassName: string;
}

/**
 * Copy for the transcript panel's empty state while a recording is running.
 *
 * #338 / #519: with the record-only provider selected there is deliberately no
 * transcript, so the default "Listening for speech… / Speak to see live
 * transcription" is actively misleading — it invites the user to wait for text
 * that will never arrive. Shared by TranscriptView and VirtualizedTranscriptView
 * so the two cannot drift.
 */
export function useTranscriptEmptyState(isPaused: boolean): TranscriptEmptyState {
  const { transcriptModelConfig } = useConfig();

  if (transcriptModelConfig?.provider === 'disabled') {
    return {
      title: 'Transcription is off',
      subtitle: 'Recording only — audio is still being saved. Pick a transcription provider in Settings to see live text.',
      // Steady, not pulsing: nothing is being listened for.
      dotClassName: 'bg-gray-400',
    };
  }

  if (isPaused) {
    return {
      title: 'Recording paused',
      subtitle: 'Click resume to continue recording',
      dotClassName: 'bg-orange-500',
    };
  }

  return {
    title: 'Listening for speech...',
    subtitle: 'Speak to see live transcription',
    dotClassName: 'bg-blue-500 animate-pulse',
  };
}
