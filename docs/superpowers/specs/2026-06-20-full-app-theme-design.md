# Full Application Theme Design

**Date:** 2026-06-20  
**Branch:** `codex-port-v04`  
**Status:** Approved for implementation planning

## Goal

Refactor the frontend design system so every user-visible surface supports light, dark, and system appearance modes. The dark theme must follow the supplied Meetily designs, while preserving visible functional accents for recording, selection, warnings, success, and informational states.

The work covers the complete frontend: application shell, navigation, settings, meeting details, editors, onboarding, model management, imports, dialogs, overlays, loading states, empty states, and secondary pages.

## Design References

The approved visual language is defined by the supplied feature screenshots:

- Dark surfaces use near-black backgrounds with slightly elevated cards.
- Borders remain visible but subdued.
- Primary text is near-white and secondary text is muted gray.
- Selection remains blue.
- Recording and destructive actions remain red.
- Warnings and transcript matches remain yellow.
- Success and ready states remain green.
- Controls and editors visually belong to the surrounding surface rather than retaining white backgrounds.

The reference assets currently live in the primary workspace under `frontend/design/feature-screenshots/`. They are design inputs, not automated test baselines.

## Non-Goals

- Do not change recording, transcription, summary, model-management, import, or database behavior.
- Do not redesign navigation structure or application layouts.
- Do not introduce additional user-selectable palettes.
- Do not replace Tailwind, Radix, BlockNote, or the existing component hierarchy.
- Do not perform unrelated component or backend refactors.

## Architecture

Implementation follows a strict dependency order.

### Stage 1: Theme Foundation

The foundation defines theme resolution, first-paint behavior, semantic tokens, and contrast rules. This stage must complete before parallel screen migration begins.

Theme mode remains:

```ts
type ThemeMode = 'light' | 'dark' | 'system';
```

The stored value remains in `localStorage` under `themeMode`. Missing or invalid values resolve to `system`.

A small pre-hydration boot script runs before the application is painted. It:

1. Reads the stored mode.
2. Reads `prefers-color-scheme: dark` when mode is `system`.
3. Toggles `dark` on `document.documentElement`.
4. Sets `document.documentElement.style.colorScheme`.

This prevents a light frame from flashing before React mounts.

`ConfigContext` remains the runtime owner of the selected mode. It updates storage, applies explicit changes immediately, and subscribes to `matchMedia` changes only while mode is `system`.

### Stage 2: Semantic Tokens and UI Primitives

Neutral feature styling must use semantic tokens rather than fixed Tailwind grays.

Required surface and content tokens:

- `background` / `foreground`
- `card` / `card-foreground`
- `popover` / `popover-foreground`
- `muted` / `muted-foreground`
- `accent` / `accent-foreground`
- `border`
- `input`
- `ring`
- `primary` / `primary-foreground`
- `secondary` / `secondary-foreground`

Required functional status families:

- `info` / `info-foreground`
- `success` / `success-foreground`
- `warning` / `warning-foreground`
- `recording` / `recording-foreground`
- existing `destructive` / `destructive-foreground`

Each status family defines a readable foreground, a subtle background treatment, and a visible border treatment for both light and dark modes.

Normal text must meet a minimum 4.5:1 contrast ratio against its surface. Large text, icons, control boundaries, and meaningful status indicators must meet at least 3:1.

Shared primitives are migrated before feature components:

- Button
- Input and input groups
- Textarea
- Select and popover content
- Dialog and alert dialog
- Tabs
- Switch
- Card-like surfaces
- Alert and status badges
- Tooltip
- Progress indicators

Feature components consume these primitives and semantic tokens. They must not recreate theme logic.

### Stage 3: Parallel Feature Migration

After the foundation and primitives are stable, independent agents may migrate these groups in parallel:

1. **Application shell**
   - Root pages
   - Sidebar
   - Main content containers
   - Navigation and search

2. **Settings and model management**
   - General, recording, transcription, summary, and beta settings
   - Model settings modal
   - Built-in, Parakeet, and Whisper model managers
   - Summary language controls

3. **Meetings and content editors**
   - Meeting details
   - Transcript panels and transcript views
   - Summary panels
   - BlockNote editors
   - Notes pages

4. **Onboarding, import, and downloads**
   - Onboarding flow
   - Setup and download steps
   - Import audio flow
   - Download progress UI

5. **Dialogs and secondary surfaces**
   - Confirmation and retranscription dialogs
   - Update and analytics dialogs
   - About and information screens
   - Status overlays, notifications, and empty states

Agents must not modify foundation or shared primitive files during parallel migration. Required foundation changes are reported back and applied centrally to prevent conflicting token definitions.

## Component Styling Rules

### Neutral Colors

Feature components must not use fixed neutral utility classes for surfaces or content, including:

- `bg-white`
- `bg-gray-*`
- `text-gray-*`
- `border-gray-*`

Fixed neutrals are allowed only for genuinely invariant media assets, such as artwork where color is part of the asset itself.

### Functional Accents

Functional color meaning is preserved:

- Red: active recording, stop action, destructive action, unrecoverable error.
- Blue: active navigation, selected model, selected tab, informational action.
- Yellow: warning, transcript search match, recoverable attention state.
- Green: ready, successful completion, valid connection.

Feature components use status tokens or paired light/dark utilities. A color cannot be accepted solely because it is visible in light mode.

### Elevation and Borders

Dark cards use token-based surfaces and borders rather than arbitrary lighter gray blocks. Dialogs, popovers, and editors use elevated surfaces that remain distinguishable from the page background.

### Editors and External UI

BlockNote receives the resolved effective theme, not the raw preference. Native controls inherit `color-scheme` from the root element. Radix portals consume semantic popover and dialog tokens.

## Data Flow

```text
localStorage themeMode
        │
        ▼
pre-hydration resolver ──► html.dark + color-scheme
        │
        ▼
ConfigContext ThemeMode
        │
        ├── explicit light/dark changes
        └── system matchMedia subscription
                │
                ▼
semantic CSS variables
        │
        ▼
shared UI primitives
        │
        ▼
feature screens and external editors
```

Invalid storage values are ignored and treated as `system`. A missing `matchMedia` API falls back to light mode without preventing application startup.

## Automated Testing

Playwright is the required acceptance layer. Visual verification is performed by the implementation agent, not delegated to the user.

### Browser Test Environment

Playwright runs the Next.js frontend without the desktop runtime. Tests install Tauri frontend mocks using `@tauri-apps/api/mocks`:

- `mockWindows('main')`
- `mockIPC(handler, { shouldMockEvents: true })`

The IPC handler returns deterministic fixtures for commands needed by each tested route. Unknown commands fail with their command name so missing fixtures cannot silently leave pages in loading states.

### Required Theme Scenarios

For every representative route:

- Render in explicit light mode.
- Render in explicit dark mode.
- Render in system mode with light system preference.
- Render in system mode with dark system preference.
- Switch modes through the settings control.
- Reload and verify persistence.
- Change the mocked system preference while in system mode and verify live updates.

Representative routes and states:

- Home with populated and empty meeting lists.
- Settings: General, Recordings, Transcription, Summary, and Beta.
- Meeting details with transcript and summary content.
- Notes page.
- Onboarding flow.
- Import audio dialog.
- Model settings and download states.
- Confirmation, update, and analytics dialogs.
- Loading, warning, error, recording, and success states.

### Visual Assertions

Playwright screenshots are captured at a stable desktop viewport for approved representative states. Assertions also verify:

- Root `dark` class matches the effective theme.
- `color-scheme` matches the effective theme.
- No horizontal page overflow.
- Critical text and status indicators are visible.
- Dialog and popover surfaces differ from their backdrop.
- No page remains indefinitely in a loading state because of an unmocked Tauri command.

Screenshot updates require explicit review against the supplied visual language.

### Static Theme Audit

A repeatable audit rejects fixed neutral surface/content utilities in feature components. The allowlist is intentionally small and documents every exception.

## Implementation Verification

The completed implementation must pass:

```bash
pnpm exec tsc --noEmit
pnpm build
pnpm test:e2e
cargo test summary:: --lib
```

It must also pass the static theme audit and leave `codex-port-v04` free of unintended changes.

## Success Criteria

The feature is complete when:

1. Every user-visible frontend surface follows light, dark, and system modes.
2. No first-paint light flash occurs when dark mode is active.
3. Functional accents remain readable in both themes.
4. Shared primitives, not feature-specific overrides, define the visual system.
5. Playwright covers representative routes and transient states with deterministic Tauri fixtures.
6. The static audit finds no unexplained fixed neutral utilities in feature components.
7. TypeScript, production build, Playwright, and relevant Rust tests pass.
