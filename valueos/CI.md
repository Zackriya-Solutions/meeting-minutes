# ValueOS Agent — CI & release publishing

Two GitHub Actions workflows in `.github/workflows/`:

| Workflow | Fires on | What it does |
|---|---|---|
| **`ci.yml`** | push (any branch) + pull requests | Build (into a real installer, uploaded as a run artifact) & test. **Secondary branches + PRs → macOS only** (fast feedback); **`main` → full matrix `ubuntu-latest` + `macos-latest` + `windows-latest`.** |
| **`publish.yml`** | manual (`workflow_dispatch`, run from `main`) | Build the release installers → **preview** the ValueOS-assigned version → **pause for your approval** → upload to S3 + register the release. |

## 1 & 2 — CI (`ci.yml`)

A tiny `set-matrix` job picks the OS list from the ref (`main` → all three, else `[macos-latest]`);
the `build` job consumes `matrix.os` and runs: checkout (submodules) → pnpm/Node + Rust toolchain +
caches → install deps → build the sidecar → **build** (`tauri build` with installer packaging) →
**verify + upload** the resulting installer as the run artifact **`valueos-agent-<platform>`** (macos
`.dmg`, windows `.msi`/`.exe`, linux `.deb`/`.AppImage`) → **test** (`npm test` in
`valueos/shell-tests`, our vitest suite; the engine/HTTP layer is mocked). Tests run on every push/PR.
CI uploads the artifact to the run for download but does **not** register a release — that is
`publish.yml`'s job. Bundles land in the **repo-root** `target/<triple>/release/bundle` (the repo root
is a Cargo workspace), which is what both workflows upload.

## 3 — Publish (`publish.yml`)

Manual, three jobs:

1. **`build`** (matrix ubuntu/macOS/windows) — builds the real installers with pinned bundle formats so
   the extensions match the ValueOS contract: **`.dmg`** (macos), **`.exe`** / NSIS (windows),
   **`.AppImage`** (linux). Uploads one artifact per platform (`valueos-agent-macos` / `-windows` /
   `-linux`).
2. **`preview`** — `GET {VALUEOS_API}/api/agent/releases/next-version` (read-only, `x-api-key`), parses
   `.result.next_version` (`YYYY.MM.DD.<seq>`), and writes it to the run **summary** so you see it before
   approving.
3. **`publish`** (`environment: agent-release`) — **GitHub pauses here for the required reviewer**
   (that's "confirm the version is correct"). On approval: configure AWS creds (region `eu-central-2`),
   download the installers, upload each to the private S3 release bucket, then
   `POST {VALUEOS_API}/api/agent/releases` with the `s3_key`/`checksum`/`size` artifacts. ValueOS assigns
   the immutable version and the POST response returns the **authoritative** version (a concurrent publish
   could bump the seq vs. the preview — expected).

> Run `publish.yml` from a green `main`. The version is assigned by ValueOS — this repo only previews +
> confirms; it never computes it.

## Required repo configuration (GitHub → Settings)

**Secrets** (Settings → Secrets and variables → Actions → Secrets):
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` — the ValueOS CI IAM user (`github_ci_user`) with
  `s3:PutObject` on the release bucket.
- `AGENT_API_KEY` — the ValueOS service `x-api-key`. (`publish.yml` reads `secrets.AGENT_API_KEY`
  directly, so the existing agent key is reused — no duplicate `API_KEY` secret needed.)

**Variables** (Settings → Secrets and variables → Actions → Variables):
- `VALUEOS_API` — the app base URL, **host only, no trailing slash, no path** (the workflow appends
  `/api/agent/releases/…`). Confirmed value: `https://d2luofz0a4v7f3.cloudfront.net`.
- `AGENT_RELEASES_BUCKET` — `va-pptx-agents-agent-releases-018326344230`.
- (AWS region `eu-central-2` is hardcoded in `publish.yml`.)

**Environment** (Settings → Environments → New environment) — this is the confirmation gate:
- Name: **`agent-release`**.
- **Required reviewers:** add **yourself**. GitHub then pauses the `publish` job until you approve.
- Optionally restrict deployment branches to `main`.

## Stopping PRs from defaulting to the upstream repo

GitHub UI setting, not a workflow change: in this fork's **Settings → General**, set the default PR base
to `value-accelerator/valueos-agent`, and check the **base repository** dropdown when opening a PR.
