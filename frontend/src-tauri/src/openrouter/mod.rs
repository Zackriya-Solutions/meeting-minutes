pub mod openrouter;
pub mod commands;

pub use openrouter::*;
// Don't re-export commands to avoid conflicts - lib.rs will import directly
