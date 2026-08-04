const MODIFIER_KEYS = new Set([
  'Control', 'Shift', 'Alt', 'Meta',
  'ctrl', 'control', 'shift', 'alt', 'option', 'meta', 'super', 'cmd', 'command',
]);

const MODIFIER_DISPLAY_MAP: Record<string, string> = {
  control: 'Ctrl',
  ctrl: 'Ctrl',
  shift: 'Shift',
  alt: 'Alt',
  option: 'Alt',
  meta: '⌘',
  super: '⌘',
  cmd: '⌘',
  command: '⌘',
};

export function validateShortcut(s: string): boolean {
  if (!s || !s.trim()) return false;
  const parts = s.split('+').map((p) => p.trim());
  if (parts.length < 2) return false;
  const modifiers = parts.slice(0, -1);
  const key = parts[parts.length - 1];
  if (!key || key.trim() === '') return false;
  if (MODIFIER_KEYS.has(key)) return false;
  return modifiers.every((m) => MODIFIER_KEYS.has(m));
}

export function formatShortcut(s: string): string {
  if (!s) return '';
  return s
    .split('+')
    .map((part) => {
      const lower = part.trim().toLowerCase();
      if (MODIFIER_DISPLAY_MAP[lower]) return MODIFIER_DISPLAY_MAP[lower];
      return part.trim().length === 1 ? part.trim().toUpperCase() : part.trim();
    })
    .join('+');
}

export const DEFAULT_RECORDING_SHORTCUT = 'Control+F8';
