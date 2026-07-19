'use client';
// VALUEOS: the SINGLE adapter over upstream's transcription capability (reuse via import,
// no reimplementation). CaptureScreen depends on this hook; tests mock THIS module so they
// need no Tauri/upstream. NOTE: the live-recording glue is exercised only in the Tauri
// build (recording needs the native backend) — verify there. We intentionally do NOT use
// upstream's useRecordingStop (it saves to the upstream DB + navigates); ValueOS does its
// own store + digest + upload in FinalizeScreen instead.
import { useCallback, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { appDataDir, join } from '@tauri-apps/api/path';
import { listen } from '@tauri-apps/api/event';
import { recordingService } from '@/services/recordingService';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useConfig } from '@/contexts/ConfigContext';
import { applyConfiguredSaveFolder } from './recordingLocation';
import { labelTranscript } from './speakerLabels';
import { reduceLive, emptyLive, type LiveState } from './liveTranscript';
import { formatTranscript, type TranscriptSegment } from './transcriptFormat';
import { TRANSCRIPT_FOLDER_KEY } from '../config/configService';

export interface RecordingController {
  isRecording: boolean;
  status: string;
  /** All recognized text joined — the authoritative text used for the upload. */
  transcriptText: string;
  /** Confirmed (final) text only — solid words in the live view. */
  confirmedText: string;
  /** The current hypothesis tail (last partial segment) — the faded/italic words. '' when
   *  the recognizer has nothing tentative in flight. */
  partialText: string;
  /** Running word count over the confirmed text (shown on the recording control bar). */
  wordCount: number;
  start: (meetingName: string) => Promise<void>;
  stop: () => Promise<string>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
}

// VALUEOS: transcript text is produced via labelTranscript (speakerLabels.ts) so speaker
// prefixes appear automatically once the stack provides a per-segment source (WS2: Me/Other).

function countWords(s: string): number {
  const t = s.trim();
  return t ? t.split(/\s+/).length : 0;
}

export function useRecordingController(): RecordingController {
  const { transcripts, transcriptsRef, clearTranscripts, flushBuffer } = useTranscripts();
  const { isRecording, status } = useRecordingState();
  const { selectedDevices } = useConfig();

  // VALUEOS WS1/WS2: capture the per-segment speaker `source` (Me/Other) from the raw
  // transcript-update event, keyed by sequence_id — the upstream Transcript object drops it.
  // Merged into the final segments in stop() so the enriched .txt + upload carry speaker labels.
  const sourceMapRef = useRef<Map<number, string>>(new Map());
  const unlistenRef = useRef<null | (() => void)>(null);
  const startedAtRef = useRef<number>(0);

  // VALUEOS: live-transcript model fed directly from the transcript-update event stream via
  // reduceLive — INTERIM (is_partial) updates replace the single preview buffer, FINAL updates
  // commit (deduped by sequence_id). We deliberately do NOT derive the live split from the
  // upstream `transcripts` array: it dedupes+appends by sequence_id, so it cannot model a
  // replacing partial stream. The upstream array remains the source for the final export (stop()).
  const [live, setLive] = useState<LiveState>(emptyLive);

  const confirmedText = useMemo(() => labelTranscript(live.committed), [live]);
  const partialText = useMemo(() => live.interim?.text?.trim() ?? '', [live]);
  // Full recognized text keyed to the upstream (final-only) array — kept for parity with the final
  // export and existing callers. The live VIEW (confirmedText/partialText above) is driven by the
  // streaming reducer instead, so preview words appear before a segment finalizes.
  const transcriptText = useMemo(() => labelTranscript(transcripts), [transcripts]);
  const wordCount = useMemo(() => countWords(confirmedText), [confirmedText]);

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
      // VALUEOS: colocate the upstream audio meeting folder with the configured transcript
      // folder. Best-effort — a failure here must never block recording (the ValueOS .txt is
      // written to the configured folder regardless, in finalizeCall).
      try {
        const folder =
          typeof localStorage !== 'undefined' ? localStorage.getItem(TRANSCRIPT_FOLDER_KEY) : null;
        await applyConfiguredSaveFolder(invoke, folder);
      } catch {
        /* keep the upstream default recordings folder if preferences can't be set */
      }
      clearTranscripts();
      setLive(emptyLive);
      // Capture speaker source per segment (Me/Other) as the engine emits it.
      sourceMapRef.current = new Map();
      startedAtRef.current = Date.now();
      try {
        unlistenRef.current = await listen<{
          sequence_id?: number;
          source?: string;
          text?: string;
          is_partial?: boolean;
          audio_start_time?: number;
        }>('transcript-update', (event) => {
          const p = event.payload;
          if (!p) return;
          // Only FINAL segments contribute speaker labels to the export (keyed by sequence_id).
          if (typeof p.sequence_id === 'number' && p.source && !p.is_partial) {
            sourceMapRef.current.set(p.sequence_id, p.source);
          }
          // Drive the live view: an interim (is_partial) replaces the preview, a final commits it.
          setLive((prev) =>
            reduceLive(prev, {
              text: p.text ?? '',
              is_partial: p.is_partial,
              sequence_id: p.sequence_id,
              source: p.source,
              audio_start_time: p.audio_start_time,
            }),
          );
        });
      } catch {
        /* no live stream without the event — the transcript still renders from the final export */
      }
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
    // Merge the captured speaker source (by sequence_id) onto each segment, then format the
    // labelled + timestamped transcript used for BOTH the file and the upload.
    const enriched: TranscriptSegment[] = finalSegments.map((t) => ({
      text: t.text,
      source: t.sequence_id != null ? sourceMapRef.current.get(t.sequence_id) : undefined,
      audio_start_time: t.audio_start_time,
      audio_end_time: t.audio_end_time,
      is_partial: t.is_partial,
      sequence_id: t.sequence_id,
    }));
    unlistenRef.current?.();
    unlistenRef.current = null;
    return formatTranscript(enriched, { startedAt: startedAtRef.current || undefined });
  }, [transcriptsRef, transcripts, flushBuffer]);

  const pause = useCallback(async () => {
    await recordingService.pauseRecording();
  }, []);
  const resume = useCallback(async () => {
    await recordingService.resumeRecording();
  }, []);

  return { isRecording, status: String(status), transcriptText, confirmedText, partialText, wordCount, start, stop, pause, resume };
}
