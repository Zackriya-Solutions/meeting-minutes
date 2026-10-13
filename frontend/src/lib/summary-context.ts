import { toast } from 'sonner';

export const SUMMARY_CONTEXT_MAX_CHARS = 4000;
export const SUMMARY_CONTEXT_COUNTER_FROM = 3600;
export const SUMMARY_CONTEXT_SAVE_DELAY_MS = 600;

/** Shared look of every summary context text area; each use adds its own min height. */
export const SUMMARY_CONTEXT_TEXTAREA_CLASS =
  'w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm resize-y disabled:bg-gray-50';

export type InheritedContextScope = 'global' | 'folder';

/** One inherited level of summary context that applies to a meeting. */
export interface InheritedContext {
  scope: InheritedContextScope;
  /** Empty for the global level, the folder id for a folder level. */
  scopeId: string;
  /** Folder name for a folder level. */
  label: string | null;
  context: string;
}

export interface MeetingSummaryContext {
  context: string;
  useInherited: boolean;
  inherited: InheritedContext[];
}

/** Name shown for an inherited level in the "Also sent" line. */
export function inheritedContextName(level: InheritedContext): string {
  if (level.scope === 'global') return 'Global context';
  return level.label ? `Folder "${level.label}"` : 'Folder context';
}

export function inheritedContextKey(level: InheritedContext): string {
  return `${level.scope}:${level.scopeId}`;
}

/** Logs a failed load or save of summary context and tells the user. */
export function reportSummaryContextError(action: 'load' | 'save', error: unknown): void {
  const message = `Failed to ${action} summary context`;
  console.error(`${message}:`, error);
  toast.error(message, { description: String(error) });
}

/** Counter text once the text is close to the limit, otherwise null. */
export function summaryContextCounter(length: number): string | null {
  if (length < SUMMARY_CONTEXT_COUNTER_FROM) return null;
  return `${length.toLocaleString('en-US')} / ${SUMMARY_CONTEXT_MAX_CHARS.toLocaleString('en-US')}`;
}
