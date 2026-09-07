/// Supported media (audio & video) file extensions for import and retranscription.
///
/// Includes native Symphonia audio formats (MP4, M4A, WAV, MP3, FLAC, OGG, AAC)
/// and FFmpeg-backed audio/video formats (MKV, WebM, WMA, MOV, AVI, WMV, M4V, FLV, 3GP, TS, etc.).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp4", "m4a", "wav", "mp3", "flac", "ogg", "aac", "mkv", "webm", "wma",
    "mov", "avi", "wmv", "m4v", "flv", "3gp", "ts", "mts", "m2ts", "ogv", "opus", "aiff"
];

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mov", "avi", "wmv", "mkv", "webm", "m4v", "flv", "3gp", "ts", "mts", "m2ts", "ogv"
];
