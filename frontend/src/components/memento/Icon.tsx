import type { SVGProps } from 'react';

export type MementoIconName = 'wave' | 'mic' | 'stop' | 'play' | 'pause' | 'transcript' | 'library' | 'search' | 'spark' | 'tag' | 'clock' | 'calendar' | 'users' | 'plus' | 'minus' | 'check' | 'check-circle' | 'chevron-right' | 'chevron-down' | 'chevron-up' | 'back' | 'close' | 'close-circle' | 'settings' | 'chat' | 'upload' | 'download' | 'home' | 'filter' | 'send' | 'alert' | 'circle' | 'copy' | 'database' | 'external' | 'eye' | 'eye-off' | 'folder' | 'globe' | 'info' | 'loader' | 'lock' | 'unlock' | 'edit' | 'pin' | 'refresh' | 'save' | 'speaker' | 'trash' | 'dot';

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
  minus: <path d="M5 12h14" />,
  check: <path d="m6 12.5 4 4 8-8.5" />,
  'check-circle': <><circle cx="12" cy="12" r="8" /><path d="m8.5 12 2.2 2.2 4.8-5" /></>,
  'chevron-right': <path d="m9 6 6 6-6 6" />,
  'chevron-down': <path d="m6 9 6 6 6-6" />,
  'chevron-up': <path d="m6 15 6-6 6 6" />,
  back: <path d="m15 18-6-6 6-6" />,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  'close-circle': <><circle cx="12" cy="12" r="8" /><path d="m9 9 6 6m0-6-6 6" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" /></>,
  chat: <path d="M5 5.5h14v10H9l-4 3v-13Z" />,
  upload: <path d="M12 16V4m-4 4 4-4 4 4M5 14v5h14v-5" />,
  download: <path d="M12 4v12m-4-4 4 4 4-4M5 19h14" />,
  home: <path d="m4 11 8-7 8 7v9h-6v-6h-4v6H4v-9Z" />,
  filter: <><path d="M6 5v14M12 5v14M18 5v14M4 8h4M10 15h4M16 10h4" /><circle cx="6" cy="8" r="2" /><circle cx="12" cy="15" r="2" /><circle cx="18" cy="10" r="2" /></>,
  send: <path d="m4 5 16 7-16 7 3-7-3-7Zm3 7h13" />,
  alert: <><path d="M12 4 3.5 19h17L12 4Z" /><path d="M12 9v4m0 3v.1" /></>,
  circle: <circle cx="12" cy="12" r="8" />,
  copy: <><rect x="8" y="8" width="11" height="11" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></>,
  database: <><ellipse cx="12" cy="6" rx="7" ry="3" /><path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" /></>,
  external: <path d="M13 5h6v6m0-6-9 9M11 7H6a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2v-5" />,
  eye: <><path d="M3 12s3.2-6 9-6 9 6 9 6-3.2 6-9 6-9-6-9-6Z" /><circle cx="12" cy="12" r="2.5" /></>,
  'eye-off': <><path d="m4 4 16 16M9.8 6.3A9 9 0 0 1 12 6c5.8 0 9 6 9 6a15 15 0 0 1-2.2 3M6.2 7.5C4.1 9.3 3 12 3 12s3.2 6 9 6c.8 0 1.5-.1 2.2-.3" /></>,
  folder: <path d="M3.5 7h6l2-2h9v14h-17V7Z" />,
  globe: <><circle cx="12" cy="12" r="8" /><path d="M4 12h16M12 4c2.2 2.2 3.2 4.9 3.2 8s-1 5.8-3.2 8c-2.2-2.2-3.2-4.9-3.2-8S9.8 6.2 12 4Z" /></>,
  info: <><circle cx="12" cy="12" r="8" /><path d="M12 11v5m0-8v.1" /></>,
  loader: <path d="M12 4a8 8 0 1 1-8 8" />,
  lock: <><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3" /></>,
  unlock: <><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M9 10V7a4 4 0 0 1 7-2.6" /></>,
  edit: <path d="m5 16-1 4 4-1L19 8l-3-3L5 16Zm9-9 3 3" />,
  pin: <path d="m8 4 8 0-1 6 3 3H6l3-3-1-6Zm4 9v7" />,
  refresh: <path d="M19 8a8 8 0 1 0 .5 7M19 4v4h-4" />,
  save: <path d="M5 4h12l2 2v14H5V4Zm3 0v6h8V4M8 20v-6h8v6" />,
  speaker: <path d="M5 10h4l4-4v12l-4-4H5v-4Zm11-1a4 4 0 0 1 0 6m2-8a7 7 0 0 1 0 10" />,
  trash: <path d="M5 7h14M9 7V4h6v3m2 0-1 13H8L7 7m4 4v5m3-5v5" />,
  dot: <circle cx="12" cy="12" r="2.6" fill="currentColor" stroke="none" />,
};

interface IconProps extends SVGProps<SVGSVGElement> { name: MementoIconName; size?: number; }

export function Icon({ name, size = 20, ...props }: IconProps) {
  return <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>{glyphs[name]}</svg>;
}
