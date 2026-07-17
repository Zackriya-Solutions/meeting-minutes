// VALUEOS: tiny formatting helpers shared across the redesigned screens.

export function wordCount(s: string): number {
  const t = (s ?? '').trim();
  return t ? t.split(/\s+/).length : 0;
}

/** Relative time like "just now", "12m ago", "3h ago", "2d ago", else a date. */
export function relTime(ts: number, now: number = Date.now()): string {
  const sec = Math.max(0, Math.round((now - ts) / 1000));
  if (sec < 45) return 'just now';
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 7) return `${day}d ago`;
  return new Date(ts).toLocaleDateString();
}
