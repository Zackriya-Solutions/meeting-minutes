/**
 * Error classification helpers for meeting export operations.
 *
 * These are pure functions extracted from `useExportOperations` so they can be
 * unit-tested in isolation (the hook itself is a thin React wrapper).
 */

export type ExportErrorKind = 'cancelled' | 'permission' | 'not_found' | 'unknown';

export interface ExportError {
  kind: ExportErrorKind;
  message: string;
  attempt: number;
}

export const MAX_RETRIES = 2;

/**
 * Classify an arbitrary error value into an actionable kind so the UI can
 * decide whether to surface a retry affordance, show a permission hint, etc.
 */
export function classifyError(err: unknown, attempt: number): ExportError {
  const msg = String(err ?? 'Unknown error').toLowerCase();
  let kind: ExportErrorKind = 'unknown';

  if (msg.includes('cancelled')) {
    kind = 'cancelled';
  } else if (
    msg.includes('permission') ||
    msg.includes('denied') ||
    msg.includes('read-only') ||
    msg.includes('eacces')
  ) {
    kind = 'permission';
  } else if (msg.includes('not found') || msg.includes('no such')) {
    kind = 'not_found';
  }

  return { kind, message: String(err ?? 'Unknown error'), attempt };
}

/**
 * Decide whether a failed export attempt should be retried.
 * Cancellations are never retried; transient/unknown failures retry up to
 * MAX_RETRIES times.
 */
export function shouldRetry(error: ExportError, maxRetries = MAX_RETRIES): boolean {
  if (error.kind === 'cancelled') return false;
  return error.attempt < maxRetries;
}
