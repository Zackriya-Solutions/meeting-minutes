//! Deep Analytics report ("Аналитический отчёт").
//!
//! A multi-stage pipeline runs over a meeting's transcript: deterministic conversation
//! analytics ([`dynamics`]) plus eight DeepSeek extraction stages and one synthesis stage
//! ([`prompts`]), combined into a local meeting score and a self-contained Russian-language
//! HTML report ([`render`]). Orchestration, progress events, and persistence live in
//! [`pipeline`]; the Tauri command surface is in [`commands`].
//!
//! NOTE: the module is deliberately named `report` (not `analytics`) — `crate::analytics`
//! is the unrelated opt-in product-statistics module.

pub mod commands;
pub mod dynamics;
pub mod pipeline;
pub mod prompts;
pub mod render;
