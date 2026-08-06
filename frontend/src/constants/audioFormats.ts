/**
 * Supported audio file extensions for import and retranscription.
 * IMPORTANT: Keep in sync with Rust constant in src-tauri/src/audio/constants.rs
 *
 * Includes:
 * - Native formats: MP4, M4A, WAV, MP3, FLAC, OGG, AAC
 * - FFmpeg-backed: MKV, WebM, WMA
 */
export const AUDIO_EXTENSIONS = [
  'mp4', 'm4a', 'wav', 'mp3', 'flac', 'ogg', 'aac', 'mkv', 'webm', 'wma'
] as const;

export type AudioExtension = typeof AUDIO_EXTENSIONS[number];

export const isAudioExtension = (ext: string): ext is AudioExtension =>{
  return (AUDIO_EXTENSIONS as readonly string[]).includes(ext);
}

/**
 * Human-readable format names for display
 */
export const AUDIO_FORMAT_DISPLAY_NAMES: Record<AudioExtension, string> = {
  mp4: 'MP4',
  m4a: 'M4A',
  wav: 'WAV',
  mp3: 'MP3',
  flac: 'FLAC',
  ogg: 'OGG',
  aac: 'AAC',
  mkv: 'MKV',
  webm: 'WebM',
  wma: 'WMA',
};

/**
 * Get comma-separated list for UI display
 * Example: "MP4, M4A, WAV, MP3, FLAC, OGG, AAC, MKV, WebM, WMA"
 */
export function getAudioFormatsDisplayList(): string {
  return AUDIO_EXTENSIONS.map(ext => AUDIO_FORMAT_DISPLAY_NAMES[ext]).join(',');
}
