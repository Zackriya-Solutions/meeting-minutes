# Advanced Exports and Custom Summary Templates

**Date:** 2026-07-26
**Branch:** `feat/gpu-whisper-live-transcription`
**Status:** Approved, implementing

## Problem

Meetily can only put a meeting on the clipboard. Two capabilities listed in
`README.md:228-229` as Pro-only are missing from the app:

- **Advanced Export Options** — PDF, DOCX, and Markdown files with formatting.
- **Custom Summary Templates** — the JSON template system exists in
  `frontend/src-tauri/src/summary/templates/`, but there is no way to create or
  edit a template from the UI. Only `list`, `details`, and `validate` commands
  exist, and `api_get_template_details` returns section *titles* only.

## Design

### 1. Single source of truth: markdown

The summary is already stored as markdown (`useSummaryGeneration.ts:275`). Export
therefore has exactly one content pipeline:

```
frontend: lib/export-markdown.ts
    meeting metadata + summary markdown + transcript lines
        -> canonical markdown string
                |
                v
rust: export/
    .md   -> the string, written verbatim
    .pdf  -> pulldown-cmark -> HTML -> printpdf (from_html)
    .docx -> pulldown-cmark -> Block model -> docx-rs
```

All three formats derive from one string, so they cannot drift. The `.md` output
doubles as the reference artifact when checking the other two.

**Content assembly lives in the frontend**, not in Rust. The frontend already
fetches every transcript (`useCopyOperations.fetchAllTranscripts`) and already
holds the summary in both the markdown and the legacy section shape; duplicating
that in Rust would mean a second set of DB queries and a second legacy-format
branch. The Rust side receives a finished markdown string and is responsible only
for conversion and file writing — the part that benefits from unit tests.

### 2. Rust module `frontend/src-tauri/src/export/`

| File | Responsibility |
|---|---|
| `blocks.rs` | markdown -> `Vec<Block>` (pulldown-cmark). Used by the DOCX renderer only. |
| `pdf.rs` | markdown -> HTML -> `printpdf::PdfDocument::from_html` |
| `docx.rs` | `Vec<Block>` -> `.docx` (docx-rs) |
| `commands.rs` | Tauri command + native save dialog |

PDF goes through HTML rather than the block model because `pulldown_cmark::html`
already emits correct nested lists and GFM tables, and printpdf's `html` feature
handles wrapping, pagination, tables, headers and page numbers. A verification
spike confirmed: Turkish glyphs (`şğüöçİı ŞĞÜÖÇ`) round-trip intact, tables and
bullets render, a long document paginated to 3 pages.

DOCX needs a structured input, so `blocks.rs` exists for it:

```rust
enum Inline { Text(String), Bold(Vec<Inline>), Italic(Vec<Inline>), Code(String), Link { .. } }
enum Block  { Heading { level, inlines }, Paragraph(..), List { ordered, items }, Table { header, rows }, Code(..), Rule }
```

#### Font pool

`printpdf` scans system fonts on first render (~7.2 s measured; subsequent
renders ~0.6 s). `printpdf::html::build_font_pool` produces a reusable
`SharedFontPool`; the app builds it once into a `OnceLock` and passes it to
`from_html_with_cache`, warmed on a background thread at startup so the first
export is not the one that pays.

No font is embedded in the binary. The PDF stylesheet uses a system stack
(`"Segoe UI", "Helvetica Neue", Arial, sans-serif`); the spike verified Turkish
rendering with system fallback on Windows.

### 3. Tauri command

```rust
api_export_document(markdown: String, format: "md"|"pdf"|"docx", suggested_name: String)
    -> Result<Option<String>, String>   // Some(path) written, None if cancelled
```

The save dialog is opened from Rust with the existing
`app.dialog().file()...blocking_save_file()` pattern used by
`database/commands.rs:26`, so no new JS plugin is added. Default filename is
`<slug>-<YYYY-MM-DD>-<scope>.<ext>`.

### 4. Frontend

- `lib/export-markdown.ts` — pure functions building the canonical markdown for
  scope `summary` / `transcript` / `both`. Transcript timestamps use the existing
  recording-relative `[MM:SS]` format.
- `hooks/meeting-details/useExportOperations.ts` — invokes the command, toasts,
  analytics. Mirrors `useCopyOperations`.
- `components/MeetingDetails/ExportMenu.tsx` — `Download` dropdown, format at the
  top level and scope in a submenu.

The menu goes in `TranscriptButtonGroup`, not `SummaryUpdaterButtonGroup`: the
latter renders only when a summary exists (`SummaryPanel.tsx:268`), which would
make transcript-only export unreachable. Summary-dependent entries are disabled
when there is no summary.

### 5. Template system

`loader.rs` gains `save_custom_template`, `delete_custom_template`, and
`template_source(id) -> Builtin | Bundled | Custom`. The custom templates
directory becomes overridable so tests can point it at a temp dir.

**ID collision rule.** `loader.rs` already resolves custom before bundled before
builtin, so a custom file named `standard_meeting.json` silently shadows the
builtin. `api_save_custom_template` therefore rejects an ID that belongs to a
builtin or bundled template, with an explicit error. The editor's "Duplicate"
action generates a free ID (`standard_meeting_copy`). Hand-written JSON dropped
into the directory keeps the existing shadowing behaviour — this rule constrains
the UI only.

New commands: `api_get_template_source` (full sections plus origin and an
`editable` flag), `api_save_custom_template`, `api_delete_custom_template`.

UI in `components/SummaryTemplates/`:
- `TemplateManager.tsx` — list with origin badges; New / Edit / Duplicate /
  Delete / import / export JSON.
- `TemplateEditor.tsx` — structured form (name, description, ordered sections with
  title, instruction, format, item_format) plus a raw-JSON tab; validates through
  `api_validate_template` before saving.
- New **Templates** tab in `SettingTabs.tsx`.

Save and delete emit a `templates-changed` event; `useTemplates` listens and
refetches so the meeting-details template dropdown updates without a restart.

### 6. Verification

- `cargo test --lib export::` — 34 tests: markdown parsing (headings, tight and
  loose lists, nesting, GFM tables, emphasis, code, links), HTML escaping and
  raw-markup stripping, DOCX package validity, PDF byte output, filename
  sanitisation.
- `cargo test --lib summary::templates` — 18 tests: the save/delete lifecycle,
  the id collision rule, and id validation, run against a temp directory.
- `node --test tests/lib/export-markdown.test.mjs` — 18 tests over the markdown
  assembly. The sibling frontend tests are written for `bun:test` and bun is not
  installed on this machine, so this file uses `node:test` instead; the module
  under test has no runtime imports, so Node's type stripping loads it directly.
- `npx tsc --noEmit` and `pnpm run build` for the UI.

Two defects surfaced during this work, both now covered by regression tests:

1. **Tight list items lost their text.** CommonMark emits tight list item text
   directly inside `Item` with no wrapping paragraph. The block builder only
   collected text into an open inline frame, so every bullet in a normal summary
   would have rendered empty in DOCX. Items now open their own inline frame,
   flushed ahead of any nested block so `- outer / - inner` keeps its order.
2. **Blank transcript segments left a stray timestamp.** The blank filter ran
   after the `[MM:SS]` prefix was prepended, so no line was ever blank and an
   empty segment exported as a lone timestamp. The filter now runs on the text.

## Out of scope

Speaker identification, calendar integration, and meeting auto-join — the other
Pro features in `README.md` — are untouched.
