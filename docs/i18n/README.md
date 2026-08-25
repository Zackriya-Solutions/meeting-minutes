# Meetily i18n Contributor Guide

> For translators, PR contributors, and code reviewers.

## Translation Workflow

1. **Check the glossary first** — before translating, look up `glossary.md` for `approved` terms
2. **Propose new terms** — if a term is missing, first open a PR adding it to `glossary.md` with status `proposed`
3. **Modify JSON** — update the corresponding JSON file by namespace
4. **Add tests** — unit tests live in `frontend/tests/i18n/`
5. **Run check:i18n** — verify with `pnpm run check:i18n` for missing keys
6. **Open a PR** — labels `i18n` + `translation`; complete the i18n checklist in the PR template

## Translation Quality Review Process

For any PR that modifies `frontend/locales/zh-CN/**`:

1. Author must:
   - Run the three gates locally: `pnpm run check:i18n && pnpm test:i18n && pnpm build`
   - Confirm no `MISSING_MESSAGE` warnings in the build output
   - Verify all new English terms have been added to `glossary.md` with status `approved` (or a parallel PR adding them as `proposed`)
   - Keep brand/proper nouns in English (Whisper / Parakeet / Ollama / OpenRouter / Claude / Groq / BlackHole)
2. Reviewer must:
   - Be a Chinese native speaker
   - Verify translations read naturally (no machine-translation artifacts)
   - Verify term consistency against `glossary.md`
   - Verify no leftover English UI text in components (use `git diff` against `frontend/locales/en-US/`)
3. CI must:
   - Pass the `i18n-check` workflow (see below) before merge

## Automated CI

The `.github/workflows/i18n-check.yml` workflow runs on every push to active wave branches and on PRs to `main` / `devtest`. It performs:

1. `pnpm install --frozen-lockfile`
2. `pnpm run check:i18n` — verifies en-US / zh-CN key parity and rejects empty values
3. `pnpm test:i18n` — 15 unit tests covering message loading, locale switching, and the check script
4. `pnpm build` — production build; catches `MISSING_MESSAGE` regressions that runtime tests miss

A failed workflow blocks PR merge.

## Key Naming Convention

Format: `{namespace}.{action}.{object}`, all lowercase with underscores.

**Good examples:**
- `recording.start_button`
- `transcript.view_empty_state`
- `summary.regenerate_action`
- `errors.permission_microphone_denied`

**Bad examples:**
- `start` (lacks namespace)
- `recordingStart` (camelCase, not underscore)
- `transcript.view.empty` (overly nested)

## Adding a New Namespace

1. Create `frontend/locales/en-US/<name>.json` and `frontend/locales/zh-CN/<name>.json`
2. Update `loadMessages` in `frontend/src/i18n/request.ts` to add an import
3. Run `pnpm run check:i18n` to verify

## Adding a New Term to the Glossary

1. Add a row at the bottom of the table in `glossary.md` with status `proposed`
2. Open a PR labeled `i18n` + `glossary`; one Chinese native-speaker approval is required
3. After merge, change status to `approved`; the term is now usable in JSON
4. To retire a term, set status to `deprecated`; consumers must replace it within the next PR cycle

## Handling Missing Keys

- **Dev environment:** Console warns but does not block
- **CI:** `pnpm run check:i18n` fails; PR merge is blocked
- **Runtime fallback:** Falls back to English (see spec §4.4)

## Reviewer Requirements

- At least 1 Chinese native-speaker reviewer approval (for translation content)
- At least 1 maintainer approval (for the PR as a whole)
- Glossary changes must be linked in the JSON update PR
