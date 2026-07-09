import { invoke as tauriInvoke } from '@tauri-apps/api/core';

/** Minimal invoke signature — injectable so the readiness logic is unit-testable. */
export type InvokeFn = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * Whether the CURRENTLY CONFIGURED transcription model is ready to record with.
 *
 * Provider-aware (this was the bug: the recording gate hardcoded a Parakeet check, so
 * GigaAM/Whisper were wrongly reported "not ready"):
 *   - gigaam       → the global GigaAM model is loaded (`gigaam_status.loaded`)
 *   - localWhisper → `whisper_has_available_models`
 *   - parakeet     → `parakeet_has_available_models` (after `parakeet_init`)
 *   - salutespeech → a Sber Authorization Key is configured (`salutespeech.auth_key`)
 *   - cloud (deepgram/elevenLabs/groq/openai) → always ready (API-key based, no local model)
 *
 * `invoke` is injected (defaults to the real Tauri invoke) so tests can supply a stub.
 */
export async function isConfiguredTranscriptionModelReady(
  invoke: InvokeFn = tauriInvoke as InvokeFn,
): Promise<boolean> {
  let provider = 'parakeet';
  try {
    const cfg = await invoke<{ provider?: string } | null>('api_get_transcript_config');
    if (cfg?.provider) provider = cfg.provider;
  } catch {
    // Config unreadable → fall back to the default (parakeet).
  }

  if (provider === 'gigaam') {
    const status = await invoke<{ model_present?: boolean; loaded?: boolean }>('gigaam_status');
    return !!status?.loaded;
  }
  if (provider === 'localWhisper' || provider === 'whisper') {
    return await invoke<boolean>('whisper_has_available_models');
  }
  if (provider === 'parakeet') {
    await invoke('parakeet_init');
    return await invoke<boolean>('parakeet_has_available_models');
  }
  if (provider === 'salutespeech') {
    // Cloud streaming — ready when a Sber Authorization Key is configured.
    try {
      const s = await invoke<Record<string, string>>('get_app_settings');
      return !!(s && s['salutespeech.auth_key'] && s['salutespeech.auth_key'].length > 0);
    } catch {
      return false;
    }
  }
  // Cloud providers authenticate via API key; no local model to gate on.
  return true;
}
