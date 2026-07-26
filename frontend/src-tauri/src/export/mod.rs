//! Document export: Markdown, PDF and DOCX.
//!
//! The frontend assembles one canonical markdown string for the meeting (see
//! `src/lib/export-markdown.ts`) and hands it to [`commands::api_export_document`].
//! Every format is a rendering of that same string, so the three outputs cannot
//! drift apart.
//!
//! - Markdown is written verbatim.
//! - PDF goes markdown -> HTML -> `printpdf` (see [`pdf`]).
//! - DOCX goes markdown -> [`blocks::Block`] -> `docx-rs` (see [`docx`]).

pub mod blocks;
pub mod commands;
pub mod docx;
pub mod pdf;
