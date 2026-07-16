import { invoke as tauriInvoke } from '@tauri-apps/api/core';

/** Minimal invoke signature — injectable so the readiness logic is unit-testable. */
export type InvokeFn = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export interface TranscriptionReadiness {
  provider: string;
  ready: boolean;
  requiresLocalModel: boolean;
}

/**
 * Whether the CURRENTLY CONFIGURED transcription model is ready to record with.
 *
 * Provider-aware (this was the bug: the recording gate hardcoded a Parakeet check, so
 * GigaAM/Whisper were wrongly reported "not ready"):
 *   - gigaam       → the global GigaAM model is loaded (`gigaam_status.loaded`)
 *   - localWhisper → `whisper_has_available_models`
 *   - parakeet     → `parakeet_has_available_models` (after `parakeet_init`)
 *   - salutespeech → usable via a Sber Authorization Key OR the managed Memento gateway
 *                    (`salutespeech_is_configured`)
 *   - cloud (deepgram/elevenLabs/groq/openai) → always ready (API-key based, no local model)
 *
 * `invoke` is injected (defaults to the real Tauri invoke) so tests can supply a stub.
 */
export async function getConfiguredTranscriptionReadiness(
  invoke: InvokeFn = tauriInvoke as InvokeFn,
): Promise<TranscriptionReadiness> {
  let provider = 'parakeet';
  try {
    const cfg = await invoke<{ provider?: string } | null>('api_get_transcript_config');
    if (cfg?.provider) provider = cfg.provider;
  } catch {
    // Config unreadable → fall back to the default (parakeet).
  }

  if (provider === 'gigaam') {
    const status = await invoke<{ model_present?: boolean; loaded?: boolean }>('gigaam_status');
    return { provider, ready: !!status?.loaded, requiresLocalModel: true };
  }
  if (provider === 'localWhisper' || provider === 'whisper') {
    return {
      provider,
      ready: await invoke<boolean>('whisper_has_available_models'),
      requiresLocalModel: true,
    };
  }
  if (provider === 'parakeet') {
    await invoke('parakeet_init');
    return {
      provider,
      ready: await invoke<boolean>('parakeet_has_available_models'),
      requiresLocalModel: true,
    };
  }
  if (provider === 'salutespeech') {
    // Cloud — ready when usable via either a user Authorization Key OR the managed
    // Memento gateway. The backend resolves both (a bare auth_key check would wrongly
    // report "not ready" in the keyless managed build).
    try {
      return {
        provider,
        ready: await invoke<boolean>('salutespeech_is_configured'),
        requiresLocalModel: false,
      };
    } catch {
      return { provider, ready: false, requiresLocalModel: false };
    }
  }
  // Cloud providers authenticate via API key; no local model to gate on.
  return { provider, ready: true, requiresLocalModel: false };
}

export async function isConfiguredTranscriptionModelReady(
  invoke: InvokeFn = tauriInvoke as InvokeFn,
): Promise<boolean> {
  return (await getConfiguredTranscriptionReadiness(invoke)).ready;
}
