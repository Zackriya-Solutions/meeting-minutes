pub mod console_utils;
pub mod commands;

pub use console_utils::*;
// Don't re-export commands to avoid conflicts - lib.rs will import directly
