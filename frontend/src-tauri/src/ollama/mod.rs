pub mod ollama;
pub mod commands;

pub use ollama::*;
// Don't re-export commands to avoid conflicts - lib.rs will import directly
