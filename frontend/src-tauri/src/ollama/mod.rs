pub mod commands;
pub mod metadata;
pub mod ollama;

pub use ollama::*;
// Don't re-export commands to avoid conflicts - lib.rs will import directly
