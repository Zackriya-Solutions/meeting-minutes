/**
 * Default model names for transcription engines.
 * IMPORTANT: Keep in sync with Rust constants in src-tauri/src/config.rs
 */

/**
 * Default Whisper model for transcription when no preference is configured.
 * This is the recommended balance of accuracy and speed.
 */
export const DEFAULT_WHISPER_MODEL = 'large-v3-turbo';

/**
 * Default Parakeet model for transcription when no preference is configured.
 * This is the quantized version optimized for speed.
 */
export const DEFAULT_PARAKEET_MODEL = 'parakeet-tdt-0.6b-v3-int8';

/**
 * Default Soniox model for realtime cloud transcription.
 */
export const DEFAULT_SONIOX_REALTIME_MODEL = 'stt-rt-v5';

/**
 * Default Soniox model for imported audio and retranscription.
 */
export const DEFAULT_SONIOX_ASYNC_MODEL = 'stt-async-v5';

/**
 * Model defaults by provider type
 */
export const MODEL_DEFAULTS = {
  whisper: DEFAULT_WHISPER_MODEL,
  localWhisper: DEFAULT_WHISPER_MODEL,
  parakeet: DEFAULT_PARAKEET_MODEL,
  soniox: DEFAULT_SONIOX_REALTIME_MODEL,
} as const;
