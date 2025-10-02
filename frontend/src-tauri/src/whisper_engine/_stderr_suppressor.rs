/// PERFORMANCE: Utility to suppress verbose C library logs (whisper.cpp, Metal, GGML)
///
/// These logs come from the C layer and bypass Rust logging, cluttering output.
/// They include:
/// - `ggml_metal_init: loaded kernel_*` (Metal GPU initialization)
/// - `whisper_full_with_state: beam search: decoder 0:` (transcription debug logs)
/// - `single timestamp ending - skip entire chunk` (whisper.cpp warnings)
///
/// This suppressor redirects stderr to /dev/null during transcription to silence
/// the C library's debug output while keeping Rust's log macros working.

use std::fs::OpenOptions;
// use std::os::fd::{AsRawFd, RawFd};

// pub struct StderrSuppressor {
//     #[cfg(unix)]
//     original_stderr: Option<RawFd>,
//     #[cfg(unix)]
//     saved_stderr: Option<RawFd>,
// }

// impl StderrSuppressor {
//     /// Create a new suppressor that redirects stderr to /dev/null
//     ///
//     /// In debug mode, keeps stderr visible for debugging.
//     /// In release mode, suppresses C library logs.
//     pub fn new() -> Self {
//         // Only suppress in release mode
//         #[cfg(all(unix, not(debug_assertions)))]
//         {
//             use std::os::unix::io::AsRawFd;

//             unsafe {
//                 // Save original stderr
//                 let original_stderr = libc::dup(libc::STDERR_FILENO);

//                 if original_stderr >= 0 {
//                     // Open /dev/null
//                     if let Ok(devnull) = OpenOptions::new().write(true).open("/dev/null") {
//                         let devnull_fd = devnull.as_raw_fd();

//                         // Redirect stderr to /dev/null
//                         if libc::dup2(devnull_fd, libc::STDERR_FILENO) >= 0 {
//                             return Self {
//                                 original_stderr: Some(original_stderr),
//                                 saved_stderr: Some(devnull_fd),
//                             };
//                         }

//                         // If dup2 failed, close the dup'd stderr
//                         libc::close(original_stderr);
//                     } else {
//                         // If /dev/null open failed, close the dup'd stderr
//                         libc::close(original_stderr);
//                     }
//                 }
//             }
//         }

//         // Debug mode or suppression failed
//         Self {
//             #[cfg(unix)]
//             original_stderr: None,
//             #[cfg(unix)]
//             saved_stderr: None,
//         }
//     }
// }

// impl Drop for StderrSuppressor {
//     fn drop(&mut self) {
//         // Restore original stderr when suppressor is dropped
//         #[cfg(all(unix, not(debug_assertions)))]
//         {
//             if let Some(original) = self.original_stderr {
//                 unsafe {
//                     libc::dup2(original, libc::STDERR_FILENO);
//                     libc::close(original);
//                 }
//             }
//         }
//     }
// }