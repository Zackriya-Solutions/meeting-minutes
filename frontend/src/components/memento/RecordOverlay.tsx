import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from './Button';
import { Icon } from './Icon';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import { addMarkedMoment, getMarkedMoments } from '@/lib/markedMoments';
import {
  getDiarizationPrefs,
  setDiarizationPrefs,
  MAX_EXPECTED_SPEAKERS,
  MIN_EXPECTED_SPEAKERS,
  type RecordingDiarizationPrefs,
} from '@/lib/diarizationPrefs';
import { useT } from '@/lib/i18n';
import {
  addStandupLiveMarker,
  changeCompletedStandupUpdates,
  getStandupLiveState,
  setStandupLiveEnabled,
  setStandupTargetMinutes,
  type StandupLiveMarkerKind,
  type StandupLiveState,
} from '@/lib/standupLiveState';

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
  const [pauseBusy, setPauseBusy] = useState(false);
  const [standup, setStandup] = useState<StandupLiveState>(() =>
    getStandupLiveState(meetingId)
  );
  const timeboxWarnedRef = useRef(false);
  const [diarization, setDiarization] = useState<RecordingDiarizationPrefs>(() =>
    getDiarizationPrefs(meetingId)
  );

  const elapsed = Math.max(0, Math.floor(activeDuration ?? 0));

  // Reload any persisted marks once the meeting id becomes available (it is set
  // by the recording-started event, which may land just after this mounts).
  useEffect(() => {
    setMarks(getMarkedMoments(meetingId));
    setDiarization(getDiarizationPrefs(meetingId));
    setStandup(getStandupLiveState(meetingId));
    timeboxWarnedRef.current = false;
  }, [meetingId]);

  useEffect(() => {
    if (
      standup.enabled &&
      elapsed >= standup.targetMinutes * 60 &&
      !timeboxWarnedRef.current
    ) {
      timeboxWarnedRef.current = true;
      toast.warning(t('Standup time-box reached'), {
        description: t('Finish the status round or move deep dives to the parking lot.'),
        duration: 8000,
      });
    }
  }, [elapsed, standup.enabled, standup.targetMinutes, t]);

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

  const togglePause = useCallback(async () => {
    if (pauseBusy) return;
    setPauseBusy(true);
    try {
      if (isPaused) {
        await recordingService.resumeRecording();
      } else {
        await recordingService.pauseRecording();
      }
    } catch (error) {
      console.error('Failed to toggle pause:', error);
      toast.error(isPaused ? t('Failed to resume recording') : t('Failed to pause recording'));
    } finally {
      setPauseBusy(false);
    }
  }, [isPaused, pauseBusy, t]);

  const toggleStandup = useCallback(() => {
    setStandup(setStandupLiveEnabled(meetingId, !standup.enabled));
    timeboxWarnedRef.current = false;
  }, [meetingId, standup.enabled]);

  const cycleTarget = useCallback(() => {
    const targets = [10, 15, 20, 30];
    const current = targets.indexOf(standup.targetMinutes);
    const next = targets[(current + 1) % targets.length];
    setStandup(setStandupTargetMinutes(meetingId, next));
    timeboxWarnedRef.current = false;
  }, [meetingId, standup.targetMinutes]);

  const changeUpdates = useCallback((delta: number) => {
    setStandup(changeCompletedStandupUpdates(meetingId, delta));
  }, [meetingId]);

  const addLiveMarker = useCallback((kind: StandupLiveMarkerKind) => {
    const previous = getStandupLiveState(meetingId);
    const next = addStandupLiveMarker(meetingId, kind, elapsed);
    setStandup(next);
    if (next.markers.length === previous.markers.length) {
      toast.info(t('This moment is already marked'), {
        description: formatTime(elapsed),
        duration: 2000,
      });
      return;
    }
    setMarks(addMarkedMoment(meetingId, elapsed));
    toast.success(kind === 'parking_lot' ? t('Added to parking lot') : t('Question marked'), {
      description: formatTime(elapsed),
      duration: 2000,
    });
  }, [elapsed, meetingId, t]);

  return (
    <section className="mm-record-overlay" aria-label={t('Meeting recording')}>
      <header className="mm-record-header">
        <span className="mm-record-dot" style={isPaused ? { animation: 'none', background: 'var(--fg3)' } : undefined} />
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
              ? { borderColor: 'var(--gold-border)', color: 'var(--gold)' }
              : { borderColor: 'var(--border-strong)', color: 'var(--fg3)' }
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
              className="mm-press flex h-6 w-6 flex-none items-center justify-center rounded-full border border-[var(--border-strong)] text-[var(--fg2)] transition-colors hover:border-[var(--gold-border)] disabled:opacity-40 disabled:hover:border-[var(--border-strong)]"
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
              className="mm-press flex h-6 w-6 flex-none items-center justify-center rounded-full border border-[var(--border-strong)] text-[var(--fg2)] transition-colors hover:border-[var(--gold-border)] disabled:opacity-40 disabled:hover:border-[var(--border-strong)]"
            >
              <Icon name="plus" size={13} />
            </button>
          </div>
        )}
      </div>
      <div className="mm-record-meta">
        <button
          type="button"
          onClick={toggleStandup}
          disabled={!meetingId}
          aria-pressed={standup.enabled}
          title={t('Enable manual standup facilitation for this recording')}
          className="mm-press flex flex-none items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors disabled:opacity-40"
          style={
            standup.enabled
              ? { borderColor: 'var(--gold-border)', color: 'var(--gold)' }
              : { borderColor: 'var(--border-strong)', color: 'var(--fg3)' }
          }
        >
          <Icon name="check-circle" size={14} />
          <span>{t('Standup mode')}</span>
        </button>
        {standup.enabled && (
          <button
            type="button"
            onClick={cycleTarget}
            className="mm-press ml-auto rounded-full border border-[var(--border-strong)] px-2.5 py-1 text-xs text-[var(--fg2)]"
            title={t('Change standup time-box')}
          >
            {t('Target')} {standup.targetMinutes}:00
          </button>
        )}
      </div>
      {standup.enabled && (
        <div className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-2.5">
          <div className="mb-2 h-1.5 overflow-hidden rounded-full bg-[var(--border-subtle)]">
            <div
              className="h-full rounded-full bg-[var(--gold)] transition-[width]"
              style={{ width: `${Math.min(100, (elapsed / (standup.targetMinutes * 60)) * 100)}%` }}
            />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs text-[var(--fg2)]">
            <span>{t('Completed status updates')}: {standup.completedUpdates}</span>
            <div className="flex gap-1">
              <button
                type="button"
                disabled={standup.completedUpdates === 0}
                onClick={() => changeUpdates(-1)}
                aria-label={t('Decrease updates covered')}
                className="mm-press flex h-6 w-6 items-center justify-center rounded border border-[var(--border-strong)] disabled:opacity-40"
              >
                <Icon name="minus" size={12} />
              </button>
              <button
                type="button"
                onClick={() => changeUpdates(1)}
                aria-label={t('Mark next update covered')}
                className="mm-press flex h-6 w-6 items-center justify-center rounded border border-[var(--gold-border)] text-[var(--gold)]"
              >
                <Icon name="plus" size={12} />
              </button>
            </div>
          </div>
          <div className="mt-2 grid grid-cols-2 gap-2">
            <Button
              variant="secondary"
              size="sm"
              icon={<Icon name="pin" size={14} />}
              onClick={() => addLiveMarker('parking_lot')}
              title={t('Save this timestamp as a topic to discuss after the status round')}
            >
              {t('Park topic')}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              icon={<Icon name="alert" size={14} />}
              onClick={() => addLiveMarker('question')}
              title={t('Save this timestamp as an unresolved question')}
            >
              {t('Mark question')}
            </Button>
          </div>
        </div>
      )}
      <div className="mm-record-actions">
        <Button
          variant="secondary"
          size="sm"
          className="!flex-none w-9 !px-0"
          aria-label={isPaused ? t('Resume') : t('Pause')}
          title={isPaused ? t('Resume') : t('Pause')}
          disabled={pauseBusy}
          icon={<Icon name={isPaused ? 'play' : 'pause'} size={16} />}
          onClick={togglePause}
        />
        <Button
          variant="secondary"
          size="sm"
          icon={<Icon name="dot" size={15} className="text-[var(--gold)]" />}
          onClick={markMoment}
        >
          {t('Mark')}
        </Button>
        <Button size="sm" icon={<Icon name="stop" size={16} />} onClick={onStop}>
          {t('Finish')}
        </Button>
      </div>
    </section>
  );
}
