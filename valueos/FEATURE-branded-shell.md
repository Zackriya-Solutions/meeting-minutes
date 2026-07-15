# Feature: ValueOS Branded Shell (three-screen onboarding flow)

A ValueOS Agent-branded entry flow that launches **first**, reusing upstream's real
model-download capability, then stops before the main meeting UI:

**A. Landing** (VA branding) → **B. Model download** (rebranded, reuses upstream) →
**C. Stop page** (“Setup complete”, flow ends).

Built for **merge safety**: exactly **one** upstream file is touched (a tiny marked seam);
everything else is new code in our namespaces.

## File map (all new, except the one seam)

| Path | Role |
|------|------|
| `frontend/src/valueos/shell/ValueOsShell.tsx` | Flow state machine (A→B→C) |
| `frontend/src/valueos/shell/screens/LandingScreen.tsx` | Screen A + **login seam** |
| `frontend/src/valueos/shell/screens/ModelDownloadScreen.tsx` | Screen B — **reuses** `useOnboarding()` |
| `frontend/src/valueos/shell/screens/StopScreen.tsx` | Screen C + **hand-off seam** |
| `frontend/src/valueos/shell/flag.ts` | `valueOsShellEnabled` master switch |
| `frontend/src/valueos/shell/index.ts` | Barrel imported by the seam |
| `frontend/src/valueos/assets/VaLogo.tsx` | VA brand mark (compact SVG) |
| `frontend/src/valueos/assets/valueos-agent-logo.svg` | Full-fidelity VA logo (reference) |
| `valueos/shell-tests/**` | Self-contained vitest project (our tests) |
| `.github/workflows/valueos-tests.yml` | CI: runs our tests on push |
| `frontend/src/app/layout.tsx` | **ONLY upstream edit** — the marked seam (below) |

## Reused (imported) vs copied

- **Reused, imported as-is (no copy): model download.** Screen B calls `useOnboarding()`
  from `@/contexts/OnboardingContext` — an exported hook whose provider wraps every route.
  It drives the real Tauri downloads (Parakeet + summary model). We use:
  `startBackgroundDownloads({ includeParakeet, includeSummary, summaryModel })`,
  `parakeetProgress` / `summaryModelProgress`, `parakeet/​summaryModelDownloaded`,
  `recommendedSummaryModel`, `retryParakeetDownload()`. We only wrap it in branded UI.
- **Copied: nothing.** No `VALUEOS-COPIED-FROM` files exist. The capability was cleanly
  importable, so there is no manual-sync burden.

## Launch mechanism (how our shell shows first)

Upstream's `layout.tsx` gates the whole UI (`showOnboarding ? <OnboardingFlow/> :
<Sidebar/> + <MainContent>{children}</MainContent>`), and every route is `children`, so a
new route / Tauri-window URL cannot become the first screen on its own. The agreed,
smallest merge-safe mechanism is a **single `// VALUEOS:` seam** at that one gate:

```tsx
// import (top of layout.tsx)
// VALUEOS: branded shell entry point (single seam — see frontend/src/valueos/shell)
import { ValueOsShell, valueOsShellEnabled } from '@/valueos/shell'

// at the one gate:
{valueOsShellEnabled ? (
  <ValueOsShell />
) : showOnboarding ? (
  <OnboardingFlow onComplete={handleOnboardingComplete} />
) : (
  <div className="flex"><Sidebar/><MainContent>{children}</MainContent></div>
)}
```

It sits **inside** all providers, so the reused `useOnboarding()` works. No Tauri window
or config change is needed — the root layout now renders our shell. Toggle off with
`NEXT_PUBLIC_VALUEOS_SHELL=off` to restore stock upstream behavior. Find the edit with
`git grep VALUEOS frontend/src/app/layout.tsx`.

## The two seams (placeholders, intentionally NOT implemented)

- **Future login / subscription gate — between A and B.** In
  `ValueOsShell.tsx` (`LandingScreen`'s `onProceed`) and marked in `LandingScreen.tsx`
  (`VALUEOS SEAM — FUTURE LOGIN / SUBSCRIPTION GATE`). Today it advances directly.
- **Post-stop hand-off — after C.** In `ValueOsShell.tsx` (`StopScreen`'s `onContinue`,
  a no-op) and marked in `StopScreen.tsx` (`VALUEOS HAND-OFF`). The disabled
  “Start capturing (coming soon)” button is where the next feature will attach.

## Tests (our code only; upstream download mocked)

`valueos/shell-tests/` is a standalone vitest + React Testing Library project (kept
outside `frontend/` so upstream's `package.json` is never touched). It mocks
`@/contexts/OnboardingContext` and verifies:
1. the shell is the enabled entry point,
2. Screen A renders VA branding,
3. the proceed control advances A→B,
4. the download control calls the reused `startBackgroundDownloads` with the right args,
5. completion advances B→C and the flow ends (hand-off disabled).

Run locally: `cd valueos/shell-tests && npm install && npm test`.
CI: `.github/workflows/valueos-tests.yml` runs them on every push.

## Merge-safety summary

Only `frontend/src/app/layout.tsx` was modified (two small `// VALUEOS:`-marked hunks).
All other code is new files under `frontend/src/valueos/`, `valueos/`, and
`.github/workflows/`. No upstream component, page, asset, or `package.json` was changed.
