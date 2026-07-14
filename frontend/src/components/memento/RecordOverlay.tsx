import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from './Button';
import { Icon } from './Icon';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import { addMarkedMoment, getMarkedMoments } from '@/lib/markedMoments';
import { useT } from '@/lib/i18n';

interface RecordOverlayProps {
  title?: string;
  bars: string[];
  onStop: () => void;
  meetingId?: string | null;
}

function formatTime(totalSeconds: number): string {
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  return `${mins}:${String(secs).padStart(2, '0')}`;
}

export function RecordOverlay({ title = 'Новая встреча', bars, onStop, meetingId = null }: RecordOverlayProps) {
  // Duration and pause state come from the backend-synced context, so the timer
  // reflects the real recording (and survives navigation / remount) instead of a
  // local counter that resets to 0:00. Use activeDuration (excludes paused time)
  // to match RecordingStatusBar and the transcript audio timeline, so a marked
  // moment lands on the right transcript segment in meeting-details.
  const t = useT();
  const { activeDuration, isPaused } = useRecordingState();
  const [marks, setMarks] = useState<number[]>(() => getMarkedMoments(meetingId));
  const [pauseBusy, setPauseBusy] = useState(false);

  const elapsed = Math.max(0, Math.floor(activeDuration ?? 0));

  // Reload any persisted marks once the meeting id becomes available (it is set
  // by the recording-started event, which may land just after this mounts).
  useEffect(() => {
    setMarks(getMarkedMoments(meetingId));
  }, [meetingId]);

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

  return (
    <section className="mm-record-overlay" aria-label={t('Meeting recording')}>
      <header className="mm-record-header">
        <span className="mm-record-dot" style={isPaused ? { animation: 'none', background: 'var(--fg3)' } : undefined} />
        <span className="mm-eyebrow">{isPaused ? t('Paused') : t('Recording')}</span>
        <time className="mm-record-time">{formatTime(elapsed)}</time>
      </header>
      <div className="mm-equalizer" aria-hidden="true">
        {bars.map((height, index) => (
          <span key={index} style={{ height: isPaused ? '3px' : height }} />
        ))}
      </div>
      <div className="mm-record-meta">
        <Icon name="mic" size={15} />
        <span>{title}</span>
        {marks.length > 0 && <span className="mm-numeric">{marks.length} {t('marked')}</span>}
      </div>
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
