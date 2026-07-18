# ValueOS Agent — Local Build Guide (VA fork)

How to compile the **ValueOS Agent** desktop app (our fork of Meetily) **locally**.
This is a new, VA-specific file; it does not replace upstream's `README.md`. For the
fork's overall docs see [`valueos/README.md`](valueos/README.md); for cloud builds (no
local toolchain needed) see [`valueos/CI.md`](valueos/CI.md); for the
name/icon rebrand see [`valueos/branding/README.md`](valueos/branding/README.md).

> **You usually don't need this.** GitHub Actions builds + tests the app in the cloud on every
> push (see **Pipelines** below), and releases are cut from a manual workflow. Build locally
> only when you want to iterate fast or debug. See [CI.md](valueos/CI.md).

## Pipelines (GitHub Actions)

Two workflows live in `.github/workflows/`. This is the overview — the full detail and the repo
**Settings you must configure** (secrets, variables, the `agent-release` approval environment)
are in [`valueos/CI.md`](valueos/CI.md).

| Workflow | Trigger | What it does | Use it for |
|---|---|---|---|
| **`ci.yml`** | every `push` + every pull request | Builds the app **into a real installer** (macOS `.dmg`, windows `.msi`/`.exe`, linux `.deb`/`.AppImage`), uploads it as a downloadable run artifact (`valueos-agent-<platform>`), and runs our vitest suite. **Secondary branches / PRs → macOS only** (fast feedback); **`main` → the full matrix `ubuntu` + `macOS` + `windows`.** It does **not** publish/register a release (that's `publish.yml`). | Continuous validation + grabbing a test build — "does it still compile on the target OSes, do the tests pass, and can I download the resulting app to try it?" |
| **`publish.yml`** | **manual** (`workflow_dispatch`, run from `main`) | Builds the real installers (`.dmg` / `.exe` / `.AppImage`), **previews** the version ValueOS will assign (`YYYY.MM.DD.<seq>`), **pauses for your approval** (the `agent-release` environment), then uploads them to the private S3 bucket and registers the release with ValueOS. | Cutting a release. |

**Day-to-day**
- Push a branch or open a PR → `ci.yml` gives a **macOS** build + test result (fast), and
  attaches the built installer to the run as the **`valueos-agent-macos`** artifact
  (Actions → the run → *Artifacts*) so you can download and try that exact build.
- Merge to `main` → `ci.yml` runs the **full 3-OS** matrix (one installer artifact per OS).

**To release**
1. GitHub → **Actions** → **Publish** → **Run workflow** (from `main`; optional release notes).
2. The **preview** job prints the auto-assigned version to the run **summary** — check it.
3. Approve the **`agent-release`** environment (the required-reviewer gate) → the release is
   uploaded to S3 and registered with ValueOS.

The version is assigned by ValueOS (immutable); this repo only previews + confirms it. Once
published, the in-app self-updater serves the new build to entitled tenants
(see [`valueos/FEATURE-updater.md`](valueos/FEATURE-updater.md)).

The app is a **Tauri 2** desktop app: a Rust core (`frontend/src-tauri`) + a Next.js 14
frontend (`frontend/`), plus a `llama-helper` Rust sidecar. The repo root is a **Cargo
workspace** (`frontend/src-tauri` + `llama-helper`), so all Rust output lands in the
repo-root `target/` directory.

---

## 1. Prerequisites (macOS)

| Tool | Version | Install |
|------|---------|---------|
| Xcode Command Line Tools | latest | `xcode-select --install` |
| Homebrew | latest | https://brew.sh |
| CMake | 3.5+ | `brew install cmake` |
| Node.js | 20.x | `brew install node@20` (or nvm) |
| pnpm | **9.x** | `npm i -g pnpm@9` (or `corepack enable`) |
| Rust | stable ≥ 1.77 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

> pnpm **must be 9.x** — `frontend/pnpm-lock.yaml` is `lockfileVersion 9.0`; pnpm 8 will
> refuse it. macOS builds use Metal GPU acceleration automatically (no extra setup).

## 2. Get the code

```bash
git clone https://github.com/value-accelerator/valueos-agent.git
cd valueos-agent
git submodule update --init --recursive   # whisper.cpp submodule (optional: the Tauri
                                           # build uses whisper-rs, which vendors its own)
```

## 3. Build (macOS)

Copy-paste block (works on Apple Silicon **and** Intel — the target triple is detected):

```bash
# --- 3a. Build the llama-helper sidecar FIRST (REQUIRED) --------------------
# tauri.conf.json declares binaries/llama-helper as an externalBin, so the sidecar
# must exist, named with your host target triple, BEFORE `tauri build`.
cargo build --release -p llama-helper --features metal
TRIPLE=$(rustc -vV | sed -n 's/host: //p')          # e.g. aarch64-apple-darwin
mkdir -p frontend/src-tauri/binaries
cp target/release/llama-helper "frontend/src-tauri/binaries/llama-helper-$TRIPLE"

# --- 3b. (optional) Apply ValueOS Agent branding (name + icon) --------------
bash valueos/branding/apply-branding.sh

# --- 3c. Build the app ------------------------------------------------------
cd frontend
pnpm install
# Generate a config overlay that (a) renames the app to "ValueOS Agent" and
# (b) disables updater artifacts so no signing key is required for a dev build:
node ../valueos/branding/make-ci-config.js valueos-ci.config.json
pnpm exec tauri build --config valueos-ci.config.json

# --- 3d. (optional) restore upstream icons in your working tree -------------
cd ..
git checkout -- frontend/src-tauri/icons
```

**Output** (repo-root `target/`, workspace):
```
target/release/bundle/dmg/ValueOS Agent_<version>_<arch>.dmg
target/release/bundle/macos/ValueOS Agent.app
```

Want a **plain, un-branded upstream build**? Skip 3b, and in 3c drop the `--config` (but
then either set `TAURI_SIGNING_PRIVATE_KEY` or pass
`--config '{"bundle":{"createUpdaterArtifacts":false}}'` — see the gotcha below).

## 4. Dev mode (hot reload)

```bash
# build the sidecar once (step 3a) first, then:
cd frontend
pnpm install
pnpm run tauri:dev        # auto-detects GPU; opens the app with live reload
```
Frontend dev server runs on http://localhost:3118.

---

## 5. Gotchas (read if a build fails)

- **Sidecar not built** → `tauri build` fails resolving `binaries/llama-helper-<triple>`.
  Always do step 3a first. On a rebuild for a different arch, rename accordingly.
- **`A public key has been found, but no private key`** → `tauri.conf.json` has
  `createUpdaterArtifacts: true`, which demands `TAURI_SIGNING_PRIVATE_KEY`. For dev,
  disable it via the `--config` overlay (step 3c) — that key is the **auto-updater** key,
  *not* Apple code signing.
- **`icon.png is not RGBA`** → only if you replace icons by hand; the VA icons in
  `valueos/branding/icons/` are already RGBA.
- **CMake missing** → `whisper-rs`/`llama-cpp` compile whisper.cpp/llama.cpp via CMake;
  `brew install cmake`.
- **pnpm install fails on lockfile** → you're on pnpm 8; use pnpm 9.
- **Signing / running the result**: local macOS builds are **ad-hoc signed**
  (`signingIdentity: "-"`). They run on your own/unmanaged Mac (right-click → Open the
  first time). They will **not** run on a strict-MDM corporate Mac — that needs a real
  Developer ID signature + notarization (a separate topic).

---

## 6. Windows (brief)

Prereqs: Visual Studio Build Tools ("Desktop development with C++"), CMake, Node 20,
pnpm 9, Rust (MSVC). Then:

```powershell
# from a Git Bash / bash shell
cargo build --release -p llama-helper
cp target/release/llama-helper.exe frontend/src-tauri/binaries/llama-helper-x86_64-pc-windows-msvc.exe
bash valueos/branding/apply-branding.sh
cd frontend
pnpm install
node ../valueos/branding/make-ci-config.js valueos-ci.config.json
pnpm exec tauri build --config valueos-ci.config.json
```
Windows builds are CPU-only by default (no Vulkan/CUDA SDK needed). Output:
`target/release/bundle/{msi,nsis}/`.

## 7. Linux (brief)

Needs `build-essential`/`gcc-c++`, `cmake`, `libwebkit2gtk-4.1-dev`, and friends. See
upstream [`docs/building_in_linux.md`](docs/building_in_linux.md). GPU acceleration is
auto-detected by the repo's `build-gpu.sh`.
