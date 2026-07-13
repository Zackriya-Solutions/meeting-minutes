import { useEffect, useState } from 'react';
import { Button } from './Button';
import { Icon } from './Icon';

interface RecordOverlayProps { title?: string; bars: string[]; onStop: () => void; }

export function RecordOverlay({ title = 'Новая встреча', bars, onStop }: RecordOverlayProps) {
  const [seconds, setSeconds] = useState(0);
  const [marks, setMarks] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setSeconds(value => value + 1), 1000);
    const markMoment = () => setMarks(value => value + 1);
    window.addEventListener('memento-mark-moment', markMoment);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('memento-mark-moment', markMoment);
    };
  }, []);
  const time = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  return <section className="mm-record-overlay" aria-label="Запись встречи">
    <header className="mm-record-header"><span className="mm-record-dot" /><span className="mm-eyebrow">Запись идёт</span><time className="mm-record-time">{time}</time></header>
    <div className="mm-equalizer" aria-hidden="true">{bars.map((height, index) => <span key={index} style={{ height }} />)}</div>
    <div className="mm-record-meta"><Icon name="mic" size={15} /><span>{title}</span>{marks > 0 && <span className="mm-numeric">{marks} отмеч.</span>}</div>
    <div className="mm-record-actions"><Button variant="secondary" size="sm" icon={<Icon name="dot" size={15} className="text-[var(--gold)]" />} onClick={() => setMarks(value => value + 1)}>Момент</Button><Button size="sm" icon={<Icon name="stop" size={16} />} onClick={onStop}>Завершить</Button></div>
  </section>;
}
