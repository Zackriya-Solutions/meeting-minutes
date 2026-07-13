import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from './Button';
import { Icon } from './Icon';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';

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
  // local counter that resets to 0:00.
  const { recordingDuration, isPaused } = useRecordingState();
  const [marks, setMarks] = useState<number[]>([]);
  const [pauseBusy, setPauseBusy] = useState(false);

  const elapsed = Math.max(0, Math.floor(recordingDuration ?? 0));

  // Record a real timestamp (elapsed seconds) and persist it so it can be used
  // after the meeting, rather than incrementing a cosmetic counter.
  const markMoment = useCallback(() => {
    setMarks((prev) => {
      const next = [...prev, elapsed];
      if (meetingId) {
        try {
          localStorage.setItem(`memento:marks:${meetingId}`, JSON.stringify(next));
        } catch (error) {
          console.warn('Failed to persist marked moment:', error);
        }
      }
      return next;
    });
    toast.success('Момент отмечен', { description: formatTime(elapsed), duration: 2000 });
  }, [elapsed, meetingId]);

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
      toast.error(isPaused ? 'Не удалось продолжить запись' : 'Не удалось поставить запись на паузу');
    } finally {
      setPauseBusy(false);
    }
  }, [isPaused, pauseBusy]);

  return (
    <section className="mm-record-overlay" aria-label="Запись встречи">
      <header className="mm-record-header">
        <span className="mm-record-dot" style={isPaused ? { animation: 'none', background: 'var(--fg3)' } : undefined} />
        <span className="mm-eyebrow">{isPaused ? 'Пауза' : 'Запись идёт'}</span>
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
        {marks.length > 0 && <span className="mm-numeric">{marks.length} отмеч.</span>}
      </div>
      <div className="mm-record-actions">
        <Button
          variant="secondary"
          size="sm"
          className="!flex-none w-9 !px-0"
          aria-label={isPaused ? 'Продолжить' : 'Пауза'}
          title={isPaused ? 'Продолжить' : 'Пауза'}
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
          Момент
        </Button>
        <Button size="sm" icon={<Icon name="stop" size={16} />} onClick={onStop}>
          Завершить
        </Button>
      </div>
    </section>
  );
}
