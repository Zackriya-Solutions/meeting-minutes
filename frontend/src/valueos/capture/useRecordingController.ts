'use client';
// VALUEOS: the SINGLE adapter over upstream's transcription capability (reuse via import,
// no reimplementation). CaptureScreen depends on this hook; tests mock THIS module so they
// need no Tauri/upstream. NOTE: the live-recording glue is exercised only in the Tauri
// build (recording needs the native backend) — verify there. We intentionally do NOT use
// upstream's useRecordingStop (it saves to the upstream DB + navigates); ValueOS does its
// own store + digest + upload in FinalizeScreen instead.
import { useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
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
  const { transcripts, transcriptsRef, clearTranscripts, flushBuffer } = useTranscripts();
  const { isRecording, status } = useRecordingState();
  const { selectedDevices } = useConfig();

  // Live text — recomputed on every new segment so the capture screen can show real-time
  // recognition (drives a re-render as `transcripts` grows).
  const transcriptText = useMemo(() => joinTranscripts(transcripts), [transcripts]);

  const start = useCallback(
    async (meetingName: string) => {
      // CRITICAL: initialize the transcription engine BEFORE recording, exactly as upstream
      // useRecordingStart does. Without this the backend records audio but emits no
      // transcript segments — the meeting comes out empty ("No speech captured").
      await invoke('parakeet_init');
      const hasModels = await invoke<boolean>('parakeet_has_available_models');
      if (!hasModels) {
        throw new Error(
          'The transcription model isn’t ready yet. Finish the model download, then try again.',
        );
      }
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
    // Let the final in-flight segments arrive and flush, then read the AUTHORITATIVE ref
    // (not the closed-over `transcripts`, which is stale at call time). The ref is kept in
    // sync with the newest segments by TranscriptContext.
    await new Promise((r) => setTimeout(r, 800));
    flushBuffer?.();
    await new Promise((r) => setTimeout(r, 300)); // let the flush's state update settle into the ref
    const finalSegments = transcriptsRef?.current ?? transcripts;
    return joinTranscripts(finalSegments);
  }, [transcriptsRef, transcripts, flushBuffer]);

  return { isRecording, status: String(status), transcriptText, start, stop };
}
