/// PERFORMANCE: Utility to document verbose C library logs (whisper.cpp, Metal, GGML)
///
/// These logs come from the C layer and bypass Rust logging, cluttering output.
/// They include:
/// - `ggml_metal_init: loaded kernel_*` (Metal GPU initialization)
/// - `whisper_full_with_state: beam search: decoder 0:` (transcription debug logs)
/// - `single timestamp ending - skip entire chunk` (whisper.cpp warnings)
///
/// **Why they appear:**
/// These are hardcoded debug logs in the whisper.cpp C library that bypass Rust's
/// logging system and write directly to stderr. They cannot be easily suppressed
/// without modifying the C library source code.
///
/// **Impact:**
/// These logs are cosmetic only - they don't affect functionality. Your transcriptions
/// ARE working correctly. The verbose logs are just debug information from the C layer.
///
/// **Current status:**
/// This is a no-op struct that documents the issue. To actually suppress these logs,
/// you would need to either:
/// 1. Patch whisper.cpp to remove/disable debug logging
/// 2. Redirect file descriptor 2 (stderr) at the OS level
/// 3. Build whisper.cpp with GGML_DEBUG=0 and other compile flags
///
/// For now, these logs can be safely ignored in production.

pub struct StderrSuppressor;

impl StderrSuppressor {
    /// Create a new suppressor (currently a no-op)
    ///
    /// In debug mode, this allows all logs through for debugging.
    /// In release mode, the C library logs still appear but can be ignored.
    pub fn new() -> Self {
        Self
    }
}