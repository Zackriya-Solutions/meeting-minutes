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
 * Default Deepgram model for realtime cloud transcription.
 */
export const DEFAULT_DEEPGRAM_REALTIME_MODEL = 'nova-3';

/**
 * Default Deepgram model for imported audio and retranscription.
 */
export const DEFAULT_DEEPGRAM_ASYNC_MODEL = 'nova-3';

/**
 * Model defaults by provider type
 */
export const MODEL_DEFAULTS = {
  whisper: DEFAULT_WHISPER_MODEL,
  localWhisper: DEFAULT_WHISPER_MODEL,
  parakeet: DEFAULT_PARAKEET_MODEL,
  deepgram: DEFAULT_DEEPGRAM_REALTIME_MODEL,
} as const;

export const DEFAULT_TRANSCRIPTION_MODELS = MODEL_DEFAULTS;
