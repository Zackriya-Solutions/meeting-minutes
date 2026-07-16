'use client';
// VALUEOS: the SINGLE adapter over upstream's transcription capability (reuse via import,
// no reimplementation). CaptureScreen depends on this hook; tests mock THIS module so they
// need no Tauri/upstream. NOTE: the live-recording glue is exercised only in the Tauri
// build (recording needs the native backend) — verify there. We intentionally do NOT use
// upstream's useRecordingStop (it saves to the upstream DB + navigates); ValueOS does its
// own store + digest + upload in FinalizeScreen instead.
import { useCallback, useMemo } from 'react';
import { appDataDir, join } from '@tauri-apps/api/path';
import { recordingService } from '@/services/recordingService';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useConfig } from '@/contexts/ConfigContext';

export interface RecordingController {
  isRecording: boolean;
  status: string;
  transcriptText: string;
  start: (meetingName: string) => Promise<void>;
  stop: () => Promise<string>;
}

function joinTranscripts(transcripts: { text: string }[]): string {
  return transcripts
    .map((t) => t.text?.trim())
    .filter(Boolean)
    .join('\n');
}

export function useRecordingController(): RecordingController {
  const { transcripts, clearTranscripts, flushBuffer } = useTranscripts();
  const { isRecording, status } = useRecordingState();
  const { selectedDevices } = useConfig();

  const transcriptText = useMemo(() => joinTranscripts(transcripts), [transcripts]);

  const start = useCallback(
    async (meetingName: string) => {
      clearTranscripts();
      await recordingService.startRecordingWithDevices(
        selectedDevices?.micDevice ?? null,
        selectedDevices?.systemDevice ?? null,
        meetingName,
      );
    },
    [clearTranscripts, selectedDevices],
  );

  const stop = useCallback(async (): Promise<string> => {
    const dir = await appDataDir();
    const savePath = await join(dir, `valueos-recording-${Date.now()}.wav`);
    await recordingService.stopRecording(savePath);
    // Let late segments flush, then return the finished transcript text.
    await new Promise((r) => setTimeout(r, 500));
    flushBuffer?.();
    return joinTranscripts(transcripts);
  }, [transcripts, flushBuffer]);

  return { isRecording, status: String(status), transcriptText, start, stop };
}
