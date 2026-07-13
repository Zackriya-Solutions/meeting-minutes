import type { SVGProps } from 'react';

export type MementoIconName = 'wave' | 'mic' | 'stop' | 'play' | 'pause' | 'transcript' | 'library' | 'search' | 'spark' | 'tag' | 'clock' | 'calendar' | 'users' | 'plus' | 'check' | 'chevron-right' | 'back' | 'close' | 'settings' | 'chat' | 'upload' | 'home' | 'filter' | 'send' | 'alert' | 'dot';

const glyphs: Record<MementoIconName, React.ReactNode> = {
  wave: <><path d="M2.5 12C3.4 3.8 5.8 3.8 6.8 12c.9 7.6 2.9 7.6 3.8 0 .8-6.2 2.3-6.2 3.1 0 .6 4.2 1.7 4.2 2.3 0h2.4" /><circle cx="21" cy="12" r="1.7" fill="currentColor" stroke="none" /></>,
  mic: <><path d="M12 3.5a3 3 0 0 1 3 3V11a3 3 0 0 1-6 0V6.5a3 3 0 0 1 3-3Z" /><path d="M6 11a6 6 0 0 0 12 0M12 17v3.5" /></>,
  stop: <rect x="7.5" y="7.5" width="9" height="9" rx="2" />,
  play: <path d="M9 6.5v11l9-5.5-9-5.5Z" />,
  pause: <path d="M9 6v12m6-12v12" />,
  transcript: <path d="M5 7h14M5 12h14M5 17h8" />,
  library: <path d="m12 5 7 4-7 4-7-4 7-4Zm-7 8 7 4 7-4m-14 4 7 4 7-4" />,
  search: <><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4.5 4.5" /></>,
  spark: <path d="m12 3 1.7 4.3L18 9l-4.3 1.7L12 15l-1.7-4.3L6 9l4.3-1.7L12 3Zm6 12 .8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8L18 15Z" />,
  tag: <><path d="M4 11.5v-6A1.5 1.5 0 0 1 5.5 4h6L20 12.5 12.5 20 4 11.5Z" /><circle cx="8.5" cy="8.5" r="1.4" /></>,
  clock: <><circle cx="12" cy="12" r="8" /><path d="M12 8v4l2.8 2" /></>,
  calendar: <><rect x="4" y="5.5" width="16" height="14" rx="3" /><path d="M4 10.5h16M8 3.5V7M16 3.5V7" /></>,
  users: <><circle cx="9" cy="9.5" r="3.2" /><path d="M3.5 19a5.5 5.5 0 0 1 11 0m1.3-12.4a3.2 3.2 0 0 1 0 5.8M17.5 19a5.5 5.5 0 0 0-3.4-5" /></>,
  plus: <path d="M12 5v14M5 12h14" />,
  check: <path d="m6 12.5 4 4 8-8.5" />,
  'chevron-right': <path d="m9 6 6 6-6 6" />,
  back: <path d="m15 18-6-6 6-6" />,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19 13.5v-3l-2-.7-.8-1.8.9-1.9-2.2-2.2-1.9.9-1.8-.8-.7-2h-3l-.7 2-1.8.8-1.9-.9L.9 6.1 1.8 8 1 9.8l-2 .7v3l2 .7.8 1.8-.9 1.9 2.2 2.2 1.9-.9 1.8.8.7 2h3l.7-2 1.8-.8 1.9.9 2.2-2.2-.9-1.9.8-1.8 2-.7Z" transform="translate(2) scale(.83)" /></>,
  chat: <path d="M5 5.5h14v10H9l-4 3v-13Z" />,
  upload: <path d="M12 16V4m-4 4 4-4 4 4M5 14v5h14v-5" />,
  home: <path d="m4 11 8-7 8 7v9h-6v-6h-4v6H4v-9Z" />,
  filter: <><path d="M6 5v14M12 5v14M18 5v14M4 8h4M10 15h4M16 10h4" /><circle cx="6" cy="8" r="2" /><circle cx="12" cy="15" r="2" /><circle cx="18" cy="10" r="2" /></>,
  send: <path d="m4 5 16 7-16 7 3-7-3-7Zm3 7h13" />,
  alert: <><path d="M12 4 3.5 19h17L12 4Z" /><path d="M12 9v4m0 3v.1" /></>,
  dot: <circle cx="12" cy="12" r="2.6" fill="currentColor" stroke="none" />,
};

interface IconProps extends SVGProps<SVGSVGElement> { name: MementoIconName; size?: number; }

export function Icon({ name, size = 20, ...props }: IconProps) {
  return <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>{glyphs[name]}</svg>;
}
