# Cloud Builds (GitHub Actions) — Plain-Language Guide

This explains how ValueOS Agent gets compiled **in the cloud by GitHub**, so nobody has
to build it on their own laptop. If you've never used GitHub Actions, start here.

The pipeline lives in a single file: [`.github/workflows/valueos-build.yml`](../.github/workflows/valueos-build.yml).
It is the only file we added under `.github/` — every upstream (Meetily) workflow was
left untouched, and ours has a distinct name so upstream merges never conflict with it.

---

## What it does

Whenever it runs, a quick **setup** step decides which platforms to build (see the
triggers below), then GitHub spins up a fresh machine per platform — **macOS** and/or
**Windows** — and on each one it:

1. Checks out the repository (including the `whisper.cpp` git submodule).
2. Installs the toolchain: Node.js 20 + pnpm, Rust (stable), with caching so repeat
   builds are faster.
3. Builds the `llama-helper` sidecar (a small Rust helper the app bundles).
4. Compiles the whole Tauri desktop app and packages the installers.
5. Uploads the installers as **downloadable artifacts**.
6. Writes a short **summary** onto the run page (platform, success/fail, build mode,
   version, commit, artifact name, installer files).

When both platforms build, they run independently: if the Windows build fails, the
macOS build still finishes and uploads its installer (and vice-versa).

**Outputs:**

| Platform | Installer(s) you get | Artifact name |
|----------|----------------------|---------------|
| macOS (Apple Silicon) | `.dmg` disk image + the `.app` bundle | `valueos-agent-macos` |
| Windows (x64) | `.msi` and `.exe` (NSIS) installers | `valueos-agent-windows` |

---

## How to trigger a build

The pipeline builds automatically — you rarely have to click anything:

1. **Push to any branch (including `main`).** Every push builds **all supported
   platforms** (macOS + Windows). So working on your own `feature/…` branch still gets
   you full builds — you don't have to merge to `main` first.
2. **macOS-only quick test.** Push the fixed tag **`macos-test`** to build **just
   macOS** (much faster — no waiting on Windows). It's reusable, so re-run it by
   force-updating the tag:
   ```bash
   git tag -f macos-test
   git push -f origin macos-test
   ```
3. **Version tag.** Pushing a tag like `v0.4.1` builds all platforms — handy later for
   builds tied to a specific release version.
4. **Manual run.** Actions tab → **"ValueOS Agent — Build (macOS + Windows)"** →
   **"Run workflow"** → optionally choose **all / macos / windows** → **Run workflow**.
   (The manual button only appears once the workflow file is on the default branch,
   `main`.)

> Building every branch push on both platforms uses CI minutes (Windows builds are
> slow). If that becomes a concern, narrow the `branches:` list in the workflow (e.g. to
> `feature/**`) or lean on the `macos-test` tag for quick iterations.

---

## Where to find & download the installers

1. On GitHub, open the **Actions** tab.
2. Click the workflow run you care about (the most recent is at the top).
3. Scroll to the bottom of the run's **Summary** page to the **Artifacts** section.
4. Click `valueos-agent-macos` or `valueos-agent-windows` to download a `.zip`
   containing that platform's installer(s).

> Artifacts are kept for **30 days**, then GitHub deletes them automatically. Re-run the
> workflow (or push again) to produce fresh ones.

---

## Where to read the build summary

On the same run page (**Actions** → click the run), the **Summary** tab shows a
human-readable report for each platform: whether it succeeded, the app version, the
commit it was built from, and the list of installer files produced. You don't need to
open the raw logs unless something failed.

---

## A note on speed

The **first** macOS or Windows build is slow — often **20–40 minutes** — because the
cloud machine compiles a large Rust/C++ codebase (Whisper, llama, ffmpeg, etc.) from
scratch. Later runs are much faster because the pipeline **caches** the pnpm package
store and the Rust build outputs between runs. If you change a lot of Rust dependencies,
the cache is partly invalidated and that run will be slower again. Be patient with the
first green build.

---

## Signing (important) — these are UNSIGNED dev builds

Right now the pipeline produces **unsigned developer builds**, on purpose:

- **macOS:** the app is *ad-hoc* signed (no Apple Developer certificate, no
  notarization). When you open it, macOS Gatekeeper will warn you; right-click →
  **Open** (or allow it in System Settings → Privacy & Security) to run it.
- **Windows:** the installers are unsigned. Windows SmartScreen may show a
  "Windows protected your PC" warning; choose **More info → Run anyway**.

We deliberately do **not** store any signing certificates or secrets in this repo, and
the pipeline needs none to run. Proper code signing (Apple Developer ID + notarization,
and a Windows Authenticode certificate) can be added later by supplying the relevant
secrets in the repo settings and enabling the signing steps — the upstream workflows
(`build-macos.yml`, `build-windows.yml`) show how that is done and can be used as a
reference. That is a separate, deliberate step and is out of scope for this dev pipeline.

---

## What about Whisper models?

Nothing to do here. The **build** only needs FFmpeg, which the Rust build step downloads
automatically. The speech-to-text **models** (Whisper / Parakeet) are downloaded by the
app itself **at runtime**, the first time a user needs them — they are not required to
compile the app, so CI does not fetch them.

---

## Relationship to upstream CI (merge safety)

Meetily (upstream) already ships its own workflows under `.github/workflows/`
(`build-macos.yml`, `build-windows.yml`, `build.yml`, `release.yml`, and more). We left
**all of them untouched**. GitHub runs *every* workflow file it finds, so both upstream's
and ours can coexist; ours is named `valueos-build.yml` specifically so an upstream merge
never overwrites or conflicts with it. If you don't want upstream's workflows running in
our fork, disable them from the **Actions** tab (per-workflow **"Disable workflow"**) —
that's a settings-level toggle, not a file edit, so it also stays merge-safe.

See [MERGE-STRATEGY.md](MERGE-STRATEGY.md) for the general policy.
