// VALUEOS: call a native `valueos_*` command and normalize errors to ValueOsApiError.
// The Rust commands (src-tauri/src/valueos, Phase 3) reject with a JSON payload
// { status, message, scope?, feature?, fields? }; anything else becomes status 0 (treated
// as a retryable transport failure by the upload queue, never as auth/entitlement).
import { invoke } from './tauri';
import { ValueOsApiError } from '../api/types';

export interface ValueOsErrorPayload {
  status: number;
  message: string;
  scope?: string;
  feature?: string;
  fields?: Record<string, string>;
}

export async function callValueOs<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    const p = e as Partial<ValueOsErrorPayload> | undefined;
    if (p && typeof p.status === 'number') {
      throw new ValueOsApiError(p.status, p.message ?? 'ValueOS error', {
        scope: p.scope,
        feature: p.feature,
        fields: p.fields,
      });
    }
    const message = typeof e === 'string' ? e : ((e as Error)?.message ?? 'Transport error');
    throw new ValueOsApiError(0, message);
  }
}
