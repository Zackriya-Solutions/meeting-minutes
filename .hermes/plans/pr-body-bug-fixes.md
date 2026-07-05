## Description

Three small fixes extracted from the fork branch, each independently verified:

1. **fix(transcription): bind JS apiKeyVal to Rust api_key_val in transcript config** — Tauri 2's `#[tauri::command]` defaults to `ArgumentCase::Camel`, so the Rust param `api_key` was looked up as `"apiKey"` but JS sent `apiKeyVal`. Fix: rename Rust param to `api_key_val`. Caused API keys to be silently dropped (saved as NULL in DB with a "saved" toast).

2. **fix(transcription): precompute chunk_duration before closure move in drain helper** — In the paused-chunk drain path, `chunk_duration` was captured by the async closure after the chunk was already moved in. Precomputed outside the closure.

3. **fix(transcription): preserve model when saving API key for non-parakeet providers** — The API key save endpoint was resetting the model field to the default for non-Parakeet providers.

## Type of Change
- [x] Bug fixes (3)

## Testing
- [x] `cargo check` passes
- [x] `pnpm run build` compiles (11/11 pages)

## Checklist
- [x] Code follows project style
- [x] Self-reviewed
- [x] No new warnings
- [x] Existing tests pass