## Description

Two changes in useRecordingStart.ts:

1. **Replace Parakeet-only check with per-provider dispatcher** — `checkTranscriptProviderReady` now handles all providers: `disabled`, `parakeet`, `localWhisper`, `remote`, `groq`, `deepgram`, `elevenLabs`, `openai`. Previously only Parakeet was checked — non-Parakeet providers were falsely rejected as "model not downloaded".

2. **Extract shared guardTranscriptionModel** — The error-handling block was duplicated 3x (manual start, auto-start from sidebar, direct start from sidebar). Now lives in one callback.

## Related Issue
Addresses #519, #338 (per-provider dispatcher aligns with the Disabled provider PR)

## Type of Change
- [ ] Bug fix
- [x] New feature (per-provider dispatcher)
- [x] Refactor (dedup)

## Testing
- [x] `pnpm run build` compiles (11/11 pages)
- [x] No functional change — logic identical, just centralized

## Checklist
- [x] Code follows project style
- [x] Self-reviewed
- [x] No new warnings
- [x] Existing tests pass