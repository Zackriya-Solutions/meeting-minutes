'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import type { VmSegment } from './types';
import {
  getRecordingChunksProcessed,
  onTranscriptUpdate,
  pauseRecording,
  resumeRecording,
  startRecording,
  stopRecording,
  watchAudioLevels,
} from './tauriBridge';

const LEVEL_BARS = 7;

function formatClock(total: number): string {
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

function formatTs(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export function RecordingScreen({
  onStopped,
  onDiscard,
}: {
  onStopped: () => void;
  onDiscard: () => void;
}) {
  const [phase, setPhase] = useState<'starting' | 'recording' | 'stopping' | 'failed'>('starting');
  const [startError, setStartError] = useState<string | null>(null);
  const [isPaused, setIsPaused] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [segments, setSegments] = useState<VmSegment[]>([]);
  const [levels, setLevels] = useState<number[]>(Array(LEVEL_BARS).fill(0.12));
  const [autoScroll, setAutoScroll] = useState(true);
  // TEMP DIAGNOSTIC: surfaces RecordingState.stats.chunks_processed so we can
  // tell "mic isn't capturing anything" apart from "capture works but VAD /
  // transcription / save isn't" without device log access. Remove once the
  // no-transcript / no-saved-meeting issue is confirmed fixed.
  const [chunksProcessed, setChunksProcessed] = useState<number | null>(null);

  const transcriptRef = useRef<HTMLDivElement>(null);
  const pausedRef = useRef(false);
  const seqRef = useRef(0);
  const hasRealLevelsRef = useRef(false);

  // Start the native recording on mount; tear down listeners on unmount
  useEffect(() => {
    let cancelled = false;
    let unTranscript: (() => void) | undefined;
    let stopLevels: (() => void) | undefined;
    let tick: ReturnType<typeof setInterval> | undefined;
    let chunksPoll: ReturnType<typeof setInterval> | undefined;

    (async () => {
      try {
        await startRecording();
      } catch (e) {
        if (!cancelled) {
          setPhase('failed');
          setStartError(String(e));
        }
        return;
      }
      if (cancelled) return;
      setPhase('recording');

      tick = setInterval(() => {
        if (!pausedRef.current) {
          setElapsed((s) => s + 1);
          if (!hasRealLevelsRef.current) {
            // Gentle idle animation until real level data arrives
            setLevels((ls) => ls.map(() => 0.1 + Math.random() * 0.35));
          }
        }
      }, 1000);

      unTranscript = await onTranscriptUpdate((u) => {
        if (u.is_partial) return;
        const id = `seg-${u.sequence_id ?? seqRef.current++}`;
        setSegments((prev) => [
          ...prev,
          {
            id,
            text: u.text,
            timestamp:
              typeof u.audio_start_time === 'number' ? u.audio_start_time : prev.length * 5,
          },
        ]);
      });

      stopLevels = await watchAudioLevels((lv) => {
        hasRealLevelsRef.current = true;
        const bars = Array.from({ length: LEVEL_BARS }, (_, i) => lv[i % lv.length] ?? 0);
        setLevels(bars);
      });

      chunksPoll = setInterval(async () => {
        const n = await getRecordingChunksProcessed();
        if (!cancelled) setChunksProcessed(n);
      }, 1500);
    })();

    return () => {
      cancelled = true;
      tick && clearInterval(tick);
      chunksPoll && clearInterval(chunksPoll);
      unTranscript?.();
      stopLevels?.();
    };
  }, []);

  // Auto-scroll transcript while pinned to live
  useEffect(() => {
    if (autoScroll && transcriptRef.current) {
      transcriptRef.current.scrollTop = transcriptRef.current.scrollHeight;
    }
  }, [segments, autoScroll]);

  const togglePause = useCallback(async () => {
    try {
      if (pausedRef.current) {
        await resumeRecording();
        pausedRef.current = false;
        setIsPaused(false);
      } else {
        await pauseRecording();
        pausedRef.current = true;
        setIsPaused(true);
      }
    } catch (e) {
      console.warn('[vm] pause/resume failed', e);
    }
  }, []);

  const stop = useCallback(async () => {
    setPhase('stopping');
    try {
      await stopRecording();
    } catch (e) {
      console.warn('[vm] stop_recording failed', e);
    }
    onStopped();
  }, [onStopped]);

  const onScroll = useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      const el = e.currentTarget;
      const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      if (!nearBottom && autoScroll) setAutoScroll(false);
    },
    [autoScroll]
  );

  if (phase === 'failed') {
    return (
      <div className="col ac jc f1" style={{ padding: '32px 28px', textAlign: 'center', gap: 16 }}>
        <h2 style={{ fontSize: 19, fontWeight: 800, margin: 0 }}>Couldn&apos;t start recording</h2>
        <p className="muted" style={{ fontSize: 13.5, lineHeight: 1.5, margin: 0 }}>
          {startError || 'The microphone may be unavailable or permission was denied.'}
        </p>
        <button className="btn btns md" onClick={onDiscard}>
          Back
        </button>
      </div>
    );
  }

  return (
    <div className="col f1" style={{ height: '100%', position: 'relative' }}>
      <div className="row jc" style={{ padding: '14px 0 4px' }}>
        <span className="pill" style={{ background: 'hsl(var(--accent))', color: 'hsl(var(--accent-fg))' }}>
          <span className="recdot" />
          Recording locally · nothing uploaded
        </span>
      </div>

      <div className="col ac jc gap6" style={{ padding: '18px 0 4px' }}>
        <span className="mono" style={{ fontSize: 56, fontWeight: 700, letterSpacing: '-0.01em' }}>
          {formatClock(elapsed)}
        </span>
        <span className="muted fs13 fw6">
          {phase === 'starting' ? 'Starting…' : isPaused ? 'Paused' : 'Recording'}
        </span>
      </div>

      <div className="row ac jc gap6" style={{ height: 40, padding: '4px 0 18px' }}>
        {levels.map((v, i) => (
          <div
            key={i}
            style={{
              width: 6,
              borderRadius: 3,
              background: 'hsl(var(--primary))',
              height: Math.round(6 + v * 34),
              transition: 'height .15s',
            }}
          />
        ))}
      </div>

      <div className="col f1" style={{ minHeight: 0, padding: '0 18px 14px', position: 'relative' }}>
        <div
          className="card f1"
          style={{ padding: '14px 6px 14px 16px', display: 'flex', flexDirection: 'column', minHeight: 0 }}
        >
          <span className="muted fs11 fw7" style={{ padding: '0 10px 8px', letterSpacing: '0.04em' }}>
            LIVE TRANSCRIPT
          </span>
          <div className="content" ref={transcriptRef} onScroll={onScroll} style={{ paddingRight: 10 }}>
            {segments.length === 0 && (
              <p className="muted fs13" style={{ padding: '0 10px' }}>
                Listening…
                {chunksProcessed !== null && (
                  <span className="mono"> (debug: {chunksProcessed} audio chunks captured)</span>
                )}
              </p>
            )}
            {segments.slice(-100).map((seg) => (
              <div key={seg.id} className="row gap10" style={{ padding: '6px 10px', alignItems: 'flex-start' }}>
                <span className="mono muted fs11" style={{ paddingTop: 2, flexShrink: 0 }}>
                  {formatTs(seg.timestamp)}
                </span>
                <span className="fs14" style={{ lineHeight: 1.5 }}>
                  {seg.text}
                </span>
              </div>
            ))}
          </div>
        </div>
        {!autoScroll && (
          <button
            className="btn btnp sm posabs"
            style={{ bottom: 26, left: '50%', transform: 'translateX(-50%)' }}
            onClick={() => {
              setAutoScroll(true);
              const el = transcriptRef.current;
              if (el) el.scrollTop = el.scrollHeight;
            }}
          >
            ↓ Jump to live
          </button>
        )}
      </div>

      <div className="row gap12" style={{ padding: '6px 24px calc(env(safe-area-inset-bottom, 0px) + 26px)' }}>
        <button className="btn btns lg f1" onClick={togglePause} disabled={phase !== 'recording'}>
          {isPaused ? 'Resume' : 'Pause'}
        </button>
        <button className="btn btnd lg f1" onClick={stop} disabled={phase === 'stopping'}>
          {phase === 'stopping' ? 'Stopping…' : 'Stop'}
        </button>
      </div>
    </div>
  );
}
