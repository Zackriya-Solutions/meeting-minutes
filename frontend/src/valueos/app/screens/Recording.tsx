'use client';
// VALUEOS: the Recording screen (UI_GUIDE §4, §7). Full-width live transcript with the
// defining interaction: confirmed words render solid/upright; the trailing hypothesis renders
// faded + italic with a blinking blue caret. Controls: waveform (visual), running word count,
// Pause (real pause_recording), and End & upload (stop → write file → composite /calls upload).
import React, { useEffect, useRef, useState } from 'react';
import { useRecordingController } from '@/valueos/capture/useRecordingController';
import { useMicLevel } from '@/valueos/capture/useMicLevel';
import { recognitionActivity, type RecognitionActivity } from '@/valueos/capture/liveTranscript';
import type { StartCallMeta } from '../types';
import { Avatar, StatusPill } from '../parts';
import { IcMic } from '../icons';

function fmt(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function elapsedLabel(ms: number): string {
  const sec = Math.round(ms / 1000);
  if (sec < 60) return `${sec}s ago`;
  const m = Math.floor(sec / 60);
  return `${m}m ${sec % 60}s ago`;
}

export function Recording({
  meta,
  startedAt,
  hasStarted,
  onStarted,
  onEnd,
}: {
  meta: StartCallMeta;
  startedAt: number;
  /** True when this call's native capture already began (we're returning to an in-progress
   *  call after navigating away) — do NOT start it again. */
  hasStarted: boolean;
  onStarted: () => void;
  onEnd: (transcriptText: string) => Promise<void>;
}) {
  const rec = useRecordingController();
  const [phase, setPhase] = useState<'starting' | 'live' | 'ending' | 'error'>(hasStarted ? 'live' : 'starting');
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState('');
  const [now, setNow] = useState(() => Date.now());
  const liveRef = useRef<HTMLDivElement>(null);
  const startedOnce = useRef(false);

  // Live signals (fallback path — no true interim stream): a REAL VU meter from the webview's
  // own mic + an activity indicator + "seconds since the last recognized line".
  const micLevel = useMicLevel(phase === 'live' && !paused);
  const activity = recognitionActivity({
    recording: phase === 'live',
    paused,
    hasInterim: !!rec.partialText,
    speaking: micLevel > 0.08,
  });
  const lastLineRef = useRef(startedAt);
  useEffect(() => {
    lastLineRef.current = Date.now(); // a new committed line arrived → reset the "since last line" timer
  }, [rec.confirmedText]);
  const sinceLineSec = Math.max(0, Math.round((now - lastLineRef.current) / 1000));

  // Start the engine + capture once, on mount — but only if this call hasn't started yet
  // (returning to an in-progress call must not restart it).
  useEffect(() => {
    if (hasStarted || startedOnce.current) return;
    startedOnce.current = true;
    void (async () => {
      try {
        await rec.start(meta.callName);
        onStarted();
        setPhase('live');
      } catch (e) {
        setError((e as Error)?.message ?? 'Could not start recording.');
        setPhase('error');
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 1s timer (drives the running clock + the "seconds since last line" signal).
  useEffect(() => {
    if (phase !== 'live' && phase !== 'starting') return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [phase]);

  // Keep the live transcript pinned to the newest words.
  useEffect(() => {
    const el = liveRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [rec.confirmedText, rec.partialText]);

  const togglePause = async () => {
    try {
      if (paused) {
        await rec.resume();
        setPaused(false);
      } else {
        await rec.pause();
        setPaused(true);
      }
    } catch {
      /* pause/resume is best-effort; ignore transient errors */
    }
  };

  const end = async () => {
    setPhase('ending');
    try {
      const text = await rec.stop();
      await onEnd(text);
      // onEnd navigates away (this screen unmounts); nothing more to do.
    } catch (e) {
      setError((e as Error)?.message ?? 'Could not finish the upload.');
      setPhase('error');
    }
  };

  const elapsedMs = now - startedAt;

  if (phase === 'error') {
    return (
      <div className="va-page" data-testid="valueos-recording-error">
        <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 28 }}>Recording problem</h1>
        <p className="va-body" style={{ marginTop: 8 }}>{error}</p>
        <button className="va-btn va-btn-primary" style={{ marginTop: 16 }} onClick={end}>
          Try to finish &amp; upload
        </button>
      </div>
    );
  }

  return (
    <div className="va-page" data-testid="valueos-recording" style={{ maxWidth: 860 }}>
      {/* header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 14, flexWrap: 'wrap' }}>
        <StatusPill status="onair" />
        <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 26, margin: 0 }}>
          {meta.callName}
        </h1>
        <span className="va-muted" style={{ fontSize: 14 }}>
          Started {elapsedLabel(elapsedMs)}
        </span>
        <span style={{ flex: 1 }} />
        <span
          style={{ fontFamily: 'var(--font-mono)', fontSize: 22, fontWeight: 700, letterSpacing: '.02em' }}
          data-testid="valueos-recording-timer"
        >
          {fmt(elapsedMs / 1000)}
        </span>
      </div>
      <p className="va-muted" style={{ fontSize: 14, margin: '6px 0 0' }}>
        {meta.tenantName} · {meta.activityType === 'lead' ? 'Lead' : 'Opportunity'} · {meta.targetLabel} ·
        transcribing locally on this device
      </p>

      {/* control bar */}
      <div
        className="va-card"
        style={{ marginTop: 18, padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 16 }}
      >
        <VuMeter level={micLevel} paused={paused || phase !== 'live'} />
        <span className="va-muted" style={{ fontSize: 13, whiteSpace: 'nowrap' }} data-testid="valueos-recording-words">
          <strong style={{ color: 'var(--va-near-black)' }}>{rec.wordCount.toLocaleString()}</strong> words
        </span>
        <span style={{ flex: 1 }} />
        <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-recording-pause" onClick={togglePause} disabled={phase !== 'live'}>
          {paused ? 'Resume' : 'Pause'}
        </button>
        <button className="va-btn va-btn-danger va-btn-sm" data-testid="valueos-recording-end" onClick={end} disabled={phase === 'ending'}>
          {phase === 'ending' ? 'Uploading…' : 'End & upload'}
        </button>
      </div>

      {/* live transcript */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, margin: '26px 0 4px', flexWrap: 'wrap' }}>
        <div className="va-ovl">Live transcript</div>
        <ActivityChip activity={activity} secondsSinceLine={sinceLineSec} />
      </div>
      <p className="va-muted" style={{ fontSize: 13, margin: '0 0 12px' }}>
        Recognized speech appears as each phrase is confirmed. Faded, italic text is still being
        confirmed. Speaker labels (Me / Other) in the saved transcript are best-effort.
      </p>

      <div ref={liveRef} className="va-scroll" data-testid="valueos-recording-live" style={{ maxHeight: 360, overflowY: 'auto' }}>
        {phase === 'starting' && !rec.confirmedText && !rec.partialText ? (
          <div className="va-muted" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <IcMic size={16} /> Listening… recognized speech will appear here in real time.
          </div>
        ) : (
          <div style={{ display: 'flex', gap: 12 }}>
            <Avatar name="You" you />
            <div style={{ minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 4 }}>
                <span style={{ fontWeight: 700, color: 'var(--va-blue)' }}>You</span>
                <span className="va-muted" style={{ fontSize: 12 }}>{fmt(elapsedMs / 1000)}</span>
              </div>
              <p className="va-body" style={{ margin: 0, whiteSpace: 'pre-wrap', color: 'var(--va-gray-800)' }}>
                <span data-testid="valueos-recording-confirmed">{rec.confirmedText}</span>
                {rec.partialText && (
                  <>
                    {rec.confirmedText ? ' ' : ''}
                    <span
                      data-testid="valueos-recording-partial"
                      style={{ color: 'var(--va-gray-400)', fontStyle: 'italic' }}
                    >
                      {rec.partialText}
                    </span>
                  </>
                )}
                <span
                  aria-hidden="true"
                  style={{
                    display: 'inline-block',
                    width: 2,
                    height: '1.05em',
                    marginLeft: 2,
                    verticalAlign: 'text-bottom',
                    background: 'var(--va-blue)',
                    animation: 'vaBlink 1s steps(1) infinite',
                  }}
                />
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/** Real VU meter driven by the mic level (0..1). level < 0 → mic unavailable, fall back to the
 *  animated waveform so there's always a live signal. */
function VuMeter({ level, paused }: { level: number; paused: boolean }) {
  if (level < 0) return <Waveform paused={paused} />;
  const bars = 28;
  return (
    <div aria-hidden="true" data-testid="valueos-recording-vu" style={{ display: 'flex', alignItems: 'center', gap: 3, height: 30 }}>
      {Array.from({ length: bars }).map((_, i) => {
        const center = 1 - Math.abs(i - (bars - 1) / 2) / ((bars - 1) / 2); // taller in the middle
        const h = paused ? 0.12 : Math.max(0.08, Math.min(1, level * (0.55 + center * 0.9)));
        return (
          <span
            key={i}
            style={{
              width: 3,
              height: `${h * 100}%`,
              borderRadius: 2,
              background: paused ? 'var(--va-gray-200)' : 'var(--va-blue)',
              transition: 'height 80ms linear',
            }}
          />
        );
      })}
    </div>
  );
}

/** The always-on live signal: Listening / Recognizing (+ seconds since the last committed line). */
function ActivityChip({ activity, secondsSinceLine }: { activity: RecognitionActivity; secondsSinceLine: number }) {
  if (activity === 'idle') return null;
  const label = activity === 'recognizing' ? 'Recognizing…' : activity === 'paused' ? 'Paused' : 'Listening…';
  const color = activity === 'paused' ? 'var(--va-gray-400)' : activity === 'recognizing' ? 'var(--va-blue)' : 'var(--va-gray-600)';
  return (
    <span
      data-testid="valueos-recording-activity"
      data-activity={activity}
      style={{ display: 'inline-flex', alignItems: 'center', gap: 7, fontSize: 12.5, fontWeight: 700, color }}
    >
      <span
        className="va-dot"
        style={{ background: color, animation: activity === 'paused' ? 'none' : 'vaPulse 1.2s var(--ease) infinite' }}
      />
      {label}
      {activity !== 'paused' && secondsSinceLine >= 2 && (
        <span className="va-muted" style={{ fontWeight: 500 }}>· {secondsSinceLine}s since last line</span>
      )}
    </span>
  );
}

function Waveform({ paused }: { paused: boolean }) {
  const bars = Array.from({ length: 28 });
  return (
    <div aria-hidden="true" style={{ display: 'flex', alignItems: 'center', gap: 3, height: 30 }}>
      {bars.map((_, i) => (
        <span
          key={i}
          style={{
            width: 3,
            height: '100%',
            borderRadius: 2,
            transformOrigin: 'center',
            background: paused ? 'var(--va-gray-200)' : 'var(--va-blue)',
            animation: paused ? 'none' : `vaWave ${0.9 + (i % 5) * 0.12}s var(--ease) ${i * 0.045}s infinite`,
          }}
        />
      ))}
    </div>
  );
}
