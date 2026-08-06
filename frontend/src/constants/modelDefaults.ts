/**
 * Default model for transcription.
 * IMPORTANT: Keep in sync with DEFAULT_TRANSCRIBE_MODEL in src-tauri/src/config.rs
 *
 * Streaming-native and multilingual (32 locales), which the live recording path
 * requires.
 */
export const DEFAULT_TRANSCRIBE_MODEL = 'nemotron-3.5-asr-streaming-0.6b-q8';

/** The single local transcription provider. Was localWhisper / whisper / parakeet. */
export const LOCAL_PROVIDER = 'local';
