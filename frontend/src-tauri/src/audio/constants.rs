/// Supported audio file extensions for import and retranscription.
///
/// Includes native Symphonia formats (MP4, M4A, WAV, MP3, FLAC, OGG, AAC)
/// and FFmpeg-backed formats (MKV, WebM, WMA).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp4", "m4a", "wav", "mp3", "flac", "ogg", "aac", "mkv", "webm", "wma"
];

/// Apply RNNoise noise suppression to the microphone stream.
/// Disabled by default: the speech models handle background noise well.
pub const RNNOISE_APPLY_ENABLED: bool = false;
