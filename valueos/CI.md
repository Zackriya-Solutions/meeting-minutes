# ValueOS Agent — CI (plain-language guide)

ValueOS Agent uses **exactly two** GitHub Actions workflows, both in
`.github/workflows/` with `valueos-` names so they never collide with anything inherited
from upstream (Meetily). Every other workflow that came from upstream has been removed —
those were all manual (`workflow_dispatch`) and are not needed for our fork.

| Workflow | Fires on | What it does |
|---|---|---|
| **`valueos-branch.yml`** | push to any branch **except `main`**, or a manual run | Builds a **macOS** installer only. **No tests.** Fast feedback while you iterate on a feature branch. |
| **`valueos-main.yml`** | push to **`main`**, a **`v*`** tag, or a manual run | **Stage 1:** runs all ValueOS tests (must pass). **Stage 2:** builds **Linux + macOS + Windows** installers. The build only runs if the tests pass. |

## Why this split

- **Feature branches** should be quick to check — you just want to know "does it still build on my Mac?" So the branch workflow skips tests and the other two OSes.
- **`main` (and release tags)** is the quality bar — the full test suite gates the build, and we produce installers for all three desktop platforms.

## The test gate (our code only)

Stage 1 of `valueos-main.yml` runs the vitest project in `valueos/shell-tests/` (`npm ci`
then `npm test`). These test **only our code** (`frontend/src/valueos/…`) and **mock the
HTTP layer** — no live network, no ValueOS calls in CI. The `build` job declares
`needs: [test]` with `if: needs.test.result == 'success'`, so a red test suite blocks every
installer on `main`.

Tests must also **exist and pass locally** for every feature we add (run them from
`valueos/shell-tests/` with `npm test`). They just don't run in CI on feature branches.

## What the build produces

Unsigned developer installers, uploaded as run **Artifacts**:

- macOS → `valueos-agent-macos` (`.dmg` + `.app`; ad-hoc signed, not notarized)
- Windows → `valueos-agent-windows` (`.msi` / `.exe`)
- Linux → `valueos-agent-linux` (`.deb` + `.AppImage`)

Each build stamps the commit + timestamp into the app (visible on-screen) so a stale build is
obvious, forces the real ValueOS transport (`NEXT_PUBLIC_VALUEOS_REAL=on`), applies the
ValueOS branding/icons, and builds the `llama-helper` sidecar first (it's a declared
`externalBin`). No signing secrets are required: macOS ad-hoc signs and the CI overlay
(`valueos/branding/make-ci-config.js`) disables updater-artifact generation.

Adding a platform later = append one entry to the `matrix.include` list in
`valueos-main.yml` (`fail-fast: false` means one OS failing won't stop the others).

## Stopping PRs from defaulting to the upstream repo

This is a **GitHub UI setting, not a workflow change**:

1. In this fork's **Settings → General**, set the **default pull request base** to this
   repository (`value-accelerator/valueos-agent`) instead of the upstream
   (`Zackriya-Solutions/meeting-minutes`).
2. When opening a PR, also check the **base repository** dropdown at the top of the compare
   page and switch it to this fork if GitHub still preselects upstream.

Neither of these is controlled by the YAML in `.github/workflows/`.

## Deferred: publish-to-S3 CI (not built yet)

A future workflow will publish signed release artifacts to the ValueOS distribution bucket so
the in-app updater (see `valueos/FEATURE-updater.md`) can serve them. Publishing is **CI-only**
and authenticated with an `x-api-key` on the ValueOS side (not a user/agent OAuth token). It
is **intentionally not implemented here** — it's a one-paragraph placeholder pending the
publish key and the publish interface/contract. When that lands, it becomes a third
`valueos-`named workflow (triggered on `v*` tags) that uploads the built installers +
`latest.json` manifest to the bucket.
