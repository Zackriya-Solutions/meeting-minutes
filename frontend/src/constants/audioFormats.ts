/**
 * Supported audio & video file extensions for import and retranscription.
 * IMPORTANT: Keep in sync with Rust constant in src-tauri/src/audio/constants.rs
 *
 * Includes:
 * - Native formats: MP4, M4A, WAV, MP3, FLAC, OGG, AAC
 * - Video & FFmpeg-backed formats: MKV, WebM, WMA, MOV, AVI, WMV, M4V, FLV, 3GP, TS, MTS, M2TS, OGV, OPUS, AIFF
 */
export const AUDIO_EXTENSIONS = [
  'mp4', 'm4a', 'wav', 'mp3', 'flac', 'ogg', 'aac', 'mkv', 'webm', 'wma',
  'mov', 'avi', 'wmv', 'm4v', 'flv', '3gp', 'ts', 'mts', 'm2ts', 'ogv', 'opus', 'aiff'
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
  mov: 'MOV',
  avi: 'AVI',
  wmv: 'WMV',
  m4v: 'M4V',
  flv: 'FLV',
  '3gp': '3GP',
  ts: 'TS',
  mts: 'MTS',
  m2ts: 'M2TS',
  ogv: 'OGV',
  opus: 'OPUS',
  aiff: 'AIFF',
};

/**
 * Get comma-separated list for UI display
 * Example: "MP4, MOV, MKV, MP3, WAV, WebM, M4A, etc."
 */
export function getAudioFormatsDisplayList(): string {
  return 'MP4, MOV, MKV, WebM, AVI, WAV, MP3, M4A, AAC, FLAC';
}
