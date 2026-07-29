import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Icon } from './Icon';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { addMarkedMoment, getMarkedMoments } from '@/lib/markedMoments';
import {
  getDiarizationPrefs,
  setDiarizationPrefs,
  MAX_EXPECTED_SPEAKERS,
  MIN_EXPECTED_SPEAKERS,
  type RecordingDiarizationPrefs,
} from '@/lib/diarizationPrefs';
import { useT } from '@/lib/i18n';

interface RecordOverlayProps {
  title?: string;
  onStop: () => void;
  meetingId?: string | null;
}

function formatTime(totalSeconds: number): string {
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  return `${mins}:${String(secs).padStart(2, '0')}`;
}

export function RecordOverlay({ title = 'Новая встреча', onStop, meetingId = null }: RecordOverlayProps) {
  // Duration and pause state come from the backend-synced context, so the timer
  // reflects the real recording (and survives navigation / remount) instead of a
  // local counter that resets to 0:00. Use activeDuration (excludes paused time)
  // to match RecordingStatusBar and the transcript audio timeline, so a marked
  // moment lands on the right transcript segment in meeting-details.
  const t = useT();
  const { activeDuration, isPaused } = useRecordingState();
  const [marks, setMarks] = useState<number[]>(() => getMarkedMoments(meetingId));
  const [diarization, setDiarization] = useState<RecordingDiarizationPrefs>(() =>
    getDiarizationPrefs(meetingId)
  );

  const elapsed = Math.max(0, Math.floor(activeDuration ?? 0));

  // Reload any persisted marks once the meeting id becomes available (it is set
  // by the recording-started event, which may land just after this mounts).
  useEffect(() => {
    setMarks(getMarkedMoments(meetingId));
    setDiarization(getDiarizationPrefs(meetingId));
  }, [meetingId]);

  // Speaker-ID choices are written through to localStorage on every change so
  // they survive navigation/remount and are picked up by the save flow
  // (useRecordingStop → set_meeting_diarization_prefs).
  const updateDiarization = useCallback(
    (patch: Partial<RecordingDiarizationPrefs>) => {
      setDiarization((prev) => {
        const next = { ...prev, ...patch };
        setDiarizationPrefs(meetingId, next);
        return next;
      });
    },
    [meetingId]
  );

  // Stepper over Auto → 2 → … → MAX. Stepping below the floor returns to Auto.
  const stepSpeakers = useCallback(
    (delta: 1 | -1) => {
      const current = diarization.expectedSpeakers;
      const next =
        current === null
          ? delta > 0
            ? MIN_EXPECTED_SPEAKERS
            : null
          : current + delta;
      updateDiarization({
        expectedSpeakers:
          next === null || next < MIN_EXPECTED_SPEAKERS
            ? null
            : Math.min(next, MAX_EXPECTED_SPEAKERS),
      });
    },
    [diarization.expectedSpeakers, updateDiarization]
  );

  // Record a real timestamp (elapsed seconds) and persist it (keyed by meeting)
  // so meeting-details can surface it later, rather than a cosmetic counter.
  const markMoment = useCallback(() => {
    setMarks(addMarkedMoment(meetingId, elapsed));
    toast.success(t('Moment marked'), { description: formatTime(elapsed), duration: 2000 });
  }, [elapsed, meetingId, t]);

  useEffect(() => {
    const handler = () => markMoment();
    window.addEventListener('memento-mark-moment', handler);
    return () => window.removeEventListener('memento-mark-moment', handler);
  }, [markMoment]);

  return (
    <section className="mm-record-overlay" aria-label={t('Meeting recording')}>
      <header className="mm-record-header">
        <span className="mm-record-dot" style={isPaused ? { animation: 'none', background: 'hsl(var(--muted-foreground) / 0.7)' } : undefined} />
        <span className="mm-eyebrow">{isPaused ? t('Paused') : t('Recording')}</span>
        <time className="mm-record-time">{formatTime(elapsed)}</time>
      </header>
      {/* CSS-animated voice wave (see .mm-equalizer in globals.css) */}
      <div className={`mm-equalizer${isPaused ? ' is-paused' : ''}`} aria-hidden="true">
        {Array.from({ length: 7 }, (_, i) => (
          <span key={i} />
        ))}
      </div>
      <div className="mm-record-meta">
        <Icon name="mic" size={15} />
        <span>{title}</span>
        {marks.length > 0 && <span className="mm-numeric">{marks.length} {t('marked')}</span>}
      </div>
      <div className="mm-record-meta">
        <button
          type="button"
          onClick={() => updateDiarization({ speakerIdEnabled: !diarization.speakerIdEnabled })}
          aria-pressed={diarization.speakerIdEnabled}
          title={t('Detect who spoke in this meeting (runs after it ends)')}
          className="mm-press flex flex-none items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors"
          style={
            diarization.speakerIdEnabled
              ? { borderColor: 'hsl(var(--primary) / 0.4)', color: 'hsl(var(--primary))' }
              : { borderColor: 'hsl(var(--border))', color: 'hsl(var(--muted-foreground) / 0.7)' }
          }
        >
          <Icon name="users" size={14} />
          <span>{t('Speaker ID')}</span>
        </button>
        {diarization.speakerIdEnabled && (
          <div
            className="ml-auto flex items-center gap-1"
            title={t('Expected number of speakers — improves detection accuracy')}
          >
            <button
              type="button"
              onClick={() => stepSpeakers(-1)}
              disabled={diarization.expectedSpeakers === null}
              aria-label={t('Fewer speakers')}
              className="mm-press flex h-6 w-6 flex-none items-center justify-center rounded-full border border-border text-muted-foreground transition-colors hover:border-primary/40 disabled:opacity-40 disabled:hover:border-border"
            >
              <Icon name="minus" size={13} />
            </button>
            <span className="mm-numeric min-w-[38px] text-center text-xs">
              {diarization.expectedSpeakers ?? t('Auto')}
            </span>
            <button
              type="button"
              onClick={() => stepSpeakers(1)}
              disabled={diarization.expectedSpeakers === MAX_EXPECTED_SPEAKERS}
              aria-label={t('More speakers')}
              className="mm-press flex h-6 w-6 flex-none items-center justify-center rounded-full border border-border text-muted-foreground transition-colors hover:border-primary/40 disabled:opacity-40 disabled:hover:border-border"
            >
              <Icon name="plus" size={13} />
            </button>
          </div>
        )}
      </div>
      <div className="mm-record-actions">
        <Button size="sm" onClick={onStop}>
          <Icon name="stop" size={16} />
          {t('Finish')}
        </Button>
      </div>
    </section>
  );
}
