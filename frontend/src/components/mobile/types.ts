// Voice Me mobile UI — shared types.
// UI-level shapes; the tauri bridge maps real backend payloads into these.

export type VmTheme = 'light' | 'dark';
export type VmAccent = 'teal' | 'blue' | 'green' | 'amber' | 'berry';
export type VmScreen =
  | 'home'
  | 'recording'
  | 'detail'
  | 'models'
  | 'settings'
  | 'import'
  | 'recordings'
  | 'recording-detail';

export interface VmMeeting {
  id: string;
  title: string;
  created_at?: string;
  duration_s?: number;
  has_summary?: boolean;
}

export interface VmSegment {
  id: string;
  text: string;
  /** Seconds from recording start */
  timestamp: number;
}

export type VmSummaryStatus = 'idle' | 'generating' | 'ready' | 'error';

export interface VmModel {
  name: string;
  size_mb: number;
  description: string;
  recommended: boolean;
  status: 'available' | 'downloading' | 'downloaded';
  progress: number;
}

export type VmProvider = 'ondevice' | 'ollama' | 'claude' | 'groq' | 'openrouter';

export const VM_ACCENTS: { id: VmAccent; swatch: string }[] = [
  { id: 'teal', swatch: '#0f9d82' },
  { id: 'blue', swatch: '#1c7ed6' },
  { id: 'green', swatch: '#2f8a4e' },
  { id: 'amber', swatch: '#c8720f' },
  { id: 'berry', swatch: '#c23566' },
];

export const VM_TEMPLATES: { id: string; name: string }[] = [
  { id: 'standup', name: 'Standup' },
  { id: '1on1', name: '1:1' },
  { id: 'sales', name: 'Sales call' },
];

export const VM_PROVIDER_NAMES: Record<VmProvider, string> = {
  ondevice: 'On-device',
  ollama: 'Ollama (LAN)',
  claude: 'Claude',
  groq: 'Groq',
  openrouter: 'OpenRouter',
};
