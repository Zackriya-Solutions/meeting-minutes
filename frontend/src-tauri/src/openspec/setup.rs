//! First-run / on-demand setup for the OpenSpec CLI dependency chain.
//!
//! `openspec::service` (the actual "Generate OpenSpec" feature) requires either
//! a globally installed `openspec` binary, or `node`/`npx` on PATH so it can be
//! run via `npx openspec@latest`. Many users won't have either installed.
//!
//! This module provides a *proactive* setup flow (surfaced once to the user,
//! see `frontend/src/hooks/useOpenSpecSetup.ts`) that:
//!   1. Detects whether `openspec` is already functional (see
//!      `verify_openspec_functional`, not just "a file with that name exists
//!      on PATH" - see the doc comment on that function for why).
//!   2. If not, downloads a portable Node.js runtime (official nodejs.org
//!      tarball/zip, no admin/sudo required) into the app's own data
//!      directory - mirroring the `ffmpeg_sidecar` auto-install pattern
//!      already used in `audio/ffmpeg.rs`, but hand-rolled since there is no
//!      "node-sidecar" crate equivalent. This is done *unconditionally*,
//!      even if a system Node.js/npm install is already on PATH: reusing a
//!      system install was tried and reverted, because a Node.js install
//!      that was *just* performed can fail to propagate its PATH change to
//!      already-running processes on Windows until a reboot/logoff, and
//!      npm's own lifecycle-script PATH construction (see
//!      `run_npm_install_openspec`) inherits whatever PATH the calling
//!      process has - so a "found via which()" system Node.js is not a
//!      reliable guarantee that the postinstall step of `npm install -g`
//!      will actually be able to find `node`. Owning our own, fully
//!      self-contained copy removes that entire class of failure.
//!   3. Runs `npm install -g @fission-ai/openspec@latest --prefix <app dir>`
//!      using that portable npm, streaming stdout/stderr as progress events
//!      so the UI can render a console-like log.
//!   4. Prepends the resulting bin directories to the running process's PATH
//!      so the existing `which::which(...)`-based detection in
//!      `openspec::service` picks them up transparently - no changes needed
//!      there.
//!
//! The user's decision (installed / skipped) is persisted via
//! `tauri-plugin-store`, following the exact same pattern as
//! `onboarding.rs`, so the prompt is only ever shown once.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_store::StoreExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Node.js LTS version used for the portable runtime download.
/// nodejs.org keeps every released tarball available indefinitely at
/// `https://nodejs.org/dist/vX.Y.Z/`, so pinning a specific version here is
/// safe and reproducible; bump it whenever we want a newer bundled Node.
// OpenSpec's official installation guide requires Node.js 20.19.0 or newer.
// Keep the managed runtime at that minimum instead of accepting the older
// Node 20.15/20.18 versions that caused npm's EBADENGINE warning.
const NODE_VERSION: &str = "20.19.0";

const SETUP_STORE_FILE: &str = "openspec-setup.json";
const SETUP_STORE_KEY: &str = "decision";
const SETUP_PROGRESS_EVENT: &str = "openspec-setup-progress";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenSpecSetupDecision {
    /// Never asked / never resolved yet - the UI should offer setup.
    #[default]
    Unresolved,
    /// OpenSpec CLI is installed and available (system or portable).
    Installed,
    /// User explicitly dismissed the setup prompt.
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecSetupStatusPayload {
    pub decision: OpenSpecSetupDecision,
    pub node_available: bool,
    pub npm_available: bool,
    pub openspec_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRuntimeStatusPayload {
    pub node_available: bool,
    pub npm_available: bool,
    pub managed_runtime_available: bool,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Persistence (tauri-plugin-store, same pattern as onboarding.rs)
// ---------------------------------------------------------------------------

pub async fn load_setup_decision<R: Runtime>(app: &AppHandle<R>) -> OpenSpecSetupDecision {
    let store = match app.store(SETUP_STORE_FILE) {
        Ok(store) => store,
        Err(e) => {
            log::warn!("Failed to access openspec setup store: {}, defaulting to Unresolved", e);
            return OpenSpecSetupDecision::default();
        }
    };

    store
        .get(SETUP_STORE_KEY)
        .and_then(|value| serde_json::from_value::<OpenSpecSetupDecision>(value.clone()).ok())
        .unwrap_or_default()
}

pub async fn save_setup_decision<R: Runtime>(
    app: &AppHandle<R>,
    decision: OpenSpecSetupDecision,
) -> Result<(), String> {
    let store = app
        .store(SETUP_STORE_FILE)
        .map_err(|e| format!("Failed to access openspec setup store: {}", e))?;

    let value = serde_json::to_value(decision).map_err(|e| e.to_string())?;
    store.set(SETUP_STORE_KEY, value);
    store
        .save()
        .map_err(|e| format!("Failed to persist openspec setup store: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Executable name / platform helpers
// ---------------------------------------------------------------------------

pub fn node_exe_name() -> &'static str {
    if cfg!(windows) {
        "node.exe"
    } else {
        "node"
    }
}

pub fn npm_exe_name() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn node_bin_dir_within(extracted_root: &Path) -> PathBuf {
    // Windows tarballs place node.exe/npm.cmd/npx.cmd directly at the archive
    // root; Linux/macOS tarballs nest them under bin/.
    if cfg!(windows) {
        extracted_root.to_path_buf()
    } else {
        extracted_root.join("bin")
    }
}

fn npm_global_bin_dir(prefix_dir: &Path) -> PathBuf {
    // `npm install -g --prefix <dir>` puts .cmd shims directly in <dir> on
    // Windows, and under <dir>/bin on Linux/macOS.
    if cfg!(windows) {
        prefix_dir.to_path_buf()
    } else {
        prefix_dir.join("bin")
    }
}

fn node_platform_arch() -> Result<(&'static str, &'static str, &'static str), String> {
    let os = if cfg!(target_os = "windows") {
        "win"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        return Err(
            "Automatic Node.js installation is not supported on this operating system".to_string(),
        );
    };

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => {
            return Err(format!(
                "Automatic Node.js installation is not supported on architecture '{}'",
                other
            ))
        }
    };

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.xz"
    };

    Ok((os, arch, ext))
}

fn node_folder_name(os: &str, arch: &str) -> String {
    format!("node-v{}-{}-{}", NODE_VERSION, os, arch)
}

fn node_download_url(os: &str, arch: &str, ext: &str) -> String {
    format!(
        "https://nodejs.org/dist/v{version}/node-v{version}-{os}-{arch}.{ext}",
        version = NODE_VERSION,
        os = os,
        arch = arch,
        ext = ext
    )
}

fn tools_root_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?;
    Ok(app_data_dir.join("openspec-tools"))
}

/// Returns the app-managed Node.js runtime that matches the currently required
/// version. Do not reuse arbitrary older runtime directories: OpenSpec has a
/// documented minimum Node version, so accepting an old cached runtime would
/// silently recreate npm's `EBADENGINE` failure after we raise `NODE_VERSION`.
fn discover_installed_node_bin_dir(tools_root: &Path) -> Option<PathBuf> {
    let node_root = tools_root.join("node");
    let (os, arch, _) = node_platform_arch().ok()?;
    let root = node_root.join(node_folder_name(os, arch));
    let bin_dir = node_bin_dir_within(&root);
    bin_dir.join(node_exe_name()).exists().then_some(bin_dir)
}

// ---------------------------------------------------------------------------
// Process PATH management
// ---------------------------------------------------------------------------

fn prepend_process_path(dir: &Path) {
    let current = std::env::var_os("PATH").unwrap_or_default();
    if std::env::split_paths(&current).any(|p| p == dir) {
        return; // already present
    }

    let mut paths = vec![dir.to_path_buf()];
    paths.extend(std::env::split_paths(&current));

    match std::env::join_paths(paths) {
        Ok(joined) => std::env::set_var("PATH", joined),
        Err(e) => log::warn!("Failed to extend PATH with {:?}: {}", dir, e),
    }
}

fn prepend_command_path(cmd: &mut Command, dirs: &[&Path]) {
    // Windows environment variables are case-insensitive to CreateProcess,
    // but Rust's `Command::env` can preserve both `PATH` and `Path` entries.
    // npm's lifecycle runner reads the inherited `Path` spelling on Windows;
    // setting only uppercase PATH let `npm.cmd install` start but made its
    // nested `cmd /C node scripts/postinstall.js` lose node.exe. Build from
    // the canonical Windows spelling when present, then overwrite BOTH forms
    // so every layer (cmd.exe, npm and @npmcli/run-script) sees the managed
    // Node directory first.
    let current = std::env::var_os("Path")
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut paths: Vec<PathBuf> = dirs.iter().map(|d| d.to_path_buf()).collect();
    paths.extend(std::env::split_paths(&current));

    if let Ok(joined) = std::env::join_paths(paths) {
        cmd.env("PATH", &joined);
        #[cfg(windows)]
        cmd.env("Path", joined);
    }
}

/// Idempotent: if a portable Node.js and/or npm-global-installed OpenSpec
/// already exist in this app's data directory (from a previous run), prepend
/// their bin directories to the current process PATH so plain
/// `which::which("openspec")` / `which::which("node")` calls - used by both
/// this module and the existing `openspec::service` CLI detection - resolve
/// them without any further changes. Safe to call on every app startup and
/// before every setup-status check; it's just directory existence checks.
pub fn ensure_local_tools_on_path<R: Runtime>(app: &AppHandle<R>) {
    let Ok(tools_root) = tools_root_dir(app) else {
        return;
    };

    if let Some(node_bin_dir) = discover_installed_node_bin_dir(&tools_root) {
        prepend_process_path(&node_bin_dir);
    }

    let npm_bin_dir = npm_global_bin_dir(&tools_root.join("npm-global"));
    if npm_bin_dir.exists() {
        prepend_process_path(&npm_bin_dir);
    }
}

// ---------------------------------------------------------------------------
// Progress events
// ---------------------------------------------------------------------------

fn emit_progress<R: Runtime>(app: &AppHandle<R>, stage: &str, message: &str, percent: Option<f64>) {
    log::debug!("[openspec-setup:{}] {}", stage, message);
    if let Err(e) = app.emit(
        SETUP_PROGRESS_EVENT,
        serde_json::json!({
            "stage": stage,
            "message": message,
            "percent": percent,
        }),
    ) {
        log::error!("Failed to emit openspec setup progress event: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Download + extraction
// ---------------------------------------------------------------------------

async fn download_file_with_progress<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with HTTP {} for {}",
            response.status(),
            url
        ));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("Failed to create download target {:?}: {}", dest, e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded_bytes: u64 = 0;
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {}", e))?;
        downloaded_bytes += chunk.len() as u64;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write downloaded data: {}", e))?;

        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let percent = if total_bytes > 0 {
                Some((downloaded_bytes as f64 / total_bytes as f64) * 100.0)
            } else {
                None
            };
            emit_progress(
                app,
                "downloading_node",
                &format!(
                    "Downloading Node.js runtime... {:.1} / {:.1} MB",
                    downloaded_bytes as f64 / 1_048_576.0,
                    total_bytes as f64 / 1_048_576.0
                ),
                percent,
            );
            last_emit = std::time::Instant::now();
        }
    }

    emit_progress(app, "downloading_node", "Node.js download complete", Some(100.0));
    Ok(())
}

fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open downloaded archive: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;
    archive
        .extract(dest_dir)
        .map_err(|e| format!("Failed to extract zip archive: {}", e))
}

fn extract_tar_xz(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open downloaded archive: {}", e))?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest_dir)
        .map_err(|e| format!("Failed to extract tar.xz archive: {}", e))
}

// ---------------------------------------------------------------------------
// npm command construction (Windows requires .cmd shims to run via `cmd /C`)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn build_npm_command(npm_path: &Path, args: &[&str], extra_path_dirs: &[&Path]) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(npm_path);
    cmd.args(args);
    prepend_command_path(&mut cmd, extra_path_dirs);
    cmd
}

#[cfg(not(windows))]
fn build_npm_command(npm_path: &Path, args: &[&str], extra_path_dirs: &[&Path]) -> Command {
    let mut cmd = Command::new(npm_path);
    cmd.args(args);
    prepend_command_path(&mut cmd, extra_path_dirs);
    cmd
}

async fn run_npm_install_openspec<R: Runtime>(
    app: &AppHandle<R>,
    npm_bin_path: &Path,
    node_bin_dir: &Path,
    prefix_dir: &Path,
) -> Result<(), String> {
    let prefix_arg = prefix_dir.to_string_lossy().to_string();
    let npm_own_dir = npm_bin_path.parent().unwrap_or_else(|| Path::new("."));

    let mut command = build_npm_command(
        npm_bin_path,
        &[
            "install",
            "-g",
            "@fission-ai/openspec@latest",
            "--prefix",
            &prefix_arg,
            // On Windows npm delegates lifecycle scripts to `cmd /C node ...`
            // and can lose the managed node.exe from its nested environment.
            // Install dependencies deterministically first, then run
            // OpenSpec's required postinstall ourselves with the absolute
            // managed node.exe path (see `run_openspec_postinstall`).
            "--ignore-scripts",
        ],
        // `node_bin_dir` is passed explicitly (rather than assumed to be
        // `npm_bin_path`'s own parent directory) so that npm's *nested* child
        // process for the package's postinstall script (spawned internally
        // as `cmd /C node scripts/postinstall.js`) can always resolve
        // `node`, even when the system `npm` shim lives in a different
        // directory than `node.exe` (e.g. version-manager shims). This is
        // what previously caused
        // `npm error "node" no se reconoce como un comando ...` on Windows.
        &[node_bin_dir, npm_own_dir],
    );

    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start npm install: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_stdout = app.clone();
    let stdout_task = tauri::async_runtime::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_progress(&app_stdout, "installing_openspec", &line, None);
            }
        }
    });

    let app_stderr = app.clone();
    let stderr_task = tauri::async_runtime::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_progress(&app_stderr, "installing_openspec", &line, None);
            }
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("npm install did not complete: {}", e))?;

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    if !status.success() {
        return Err(format!("npm install exited with status {}", status));
    }

    Ok(())
}

async fn run_openspec_postinstall<R: Runtime>(
    app: &AppHandle<R>,
    node_bin_dir: &Path,
    prefix_dir: &Path,
) -> Result<(), String> {
    let node_path = node_bin_dir.join(node_exe_name());
    let script_path = prefix_dir
        .join("node_modules")
        .join("@fission-ai")
        .join("openspec")
        .join("scripts")
        .join("postinstall.js");

    if !script_path.exists() {
        return Err(format!(
            "OpenSpec package was downloaded but its postinstall script is missing: {}",
            script_path.display()
        ));
    }

    emit_progress(
        app,
        "installing_openspec",
        "Finalizing OpenSpec installation...",
        None,
    );

    // No shell and no bare `node` command here. The fully-qualified node.exe
    // avoids npm/cmd PATH inheritance entirely, which is the documented
    // failure mode we observed on Windows.
    let mut command = Command::new(&node_path);
    command
        .arg(&script_path)
        .current_dir(script_path.parent().expect("postinstall script parent"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    prepend_command_path(&mut command, &[node_bin_dir]);

    let output = command
        .output()
        .await
        .map_err(|e| format!("Failed to start OpenSpec postinstall: {}", e))?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        emit_progress(app, "installing_openspec", line, None);
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        emit_progress(app, "installing_openspec", line, None);
    }

    if !output.status.success() {
        return Err(format!(
            "OpenSpec postinstall failed with status {}",
            output.status
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Functional verification (existence-on-PATH is not enough - see below)
// ---------------------------------------------------------------------------

/// Actually runs the installed OpenSpec JavaScript entrypoint with the exact
/// app-managed `node.exe`, rather than checking a shim on PATH.
///
/// This matters because npm links a package's `bin` shim to disk *before*
/// running its `postinstall` lifecycle script. If that postinstall script
/// then fails, npm's bin shim may still exist. Conversely, on Windows the
/// shim runs via `cmd.exe` and can itself lose the Node runtime because of
/// `Path`/`PATH` inheritance. The direct invocation below validates the exact
/// executable pair the app will use and has no shell/PATH dependency.
pub async fn verify_openspec_functional<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Ok(tools_root) = tools_root_dir(app) else {
        return false;
    };
    let Some(node_bin_dir) = discover_installed_node_bin_dir(&tools_root) else {
        return false;
    };
    let entrypoint = tools_root
        .join("npm-global")
        .join("node_modules")
        .join("@fission-ai")
        .join("openspec")
        .join("bin")
        .join("openspec.js");
    if !entrypoint.exists() {
        return false;
    }

    let mut cmd = Command::new(node_bin_dir.join(node_exe_name()));
    cmd.arg(entrypoint).arg("--version");
    prepend_command_path(&mut cmd, &[&node_bin_dir]);

    cmd.stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return false,
    };

    matches!(
        tokio::time::timeout(Duration::from_secs(15), child.wait()).await,
        Ok(Ok(status)) if status.success()
    )
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Installs the app-managed Node.js/npm runtime only. This is deliberately a
/// separate operation from OpenSpec: it avoids relying on a Windows PATH
/// broadcast (and therefore avoids requiring a logout/restart) and gives the
/// user a verifiable prerequisite before the OpenSpec package is installed.
pub async fn install_portable_node_runtime<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    emit_progress(app, "node_runtime", "Preparing the managed Node.js runtime...", None);
    let tools_root = tools_root_dir(app)?;
    std::fs::create_dir_all(&tools_root)
        .map_err(|e| format!("Failed to create OpenSpec tools directory: {}", e))?;

    // Deliberately do NOT try to reuse a system-installed `node`/`npm`, even
    // when both resolve via `which`. This used to be an optimization, but it
    // is exactly what caused
    // `npm error "node" no se reconoce como un comando interno o externo`
    // during the OpenSpec package's postinstall script: npm's lifecycle
    // scripts inherit the PATH from whatever process spawned npm, and a
    // system Node.js install can be present yet still fail to propagate
    // correctly into a *nested* child process on Windows - most commonly
    // because Node.js was JUST installed and Windows has not yet broadcast
    // the updated PATH environment variable to already-running processes
    // (a full log-off/restart is normally required for that to reach a
    // long-lived desktop app). Relying on a fresh system install is
    // therefore inherently flaky right after installation.
    //
    // Always downloading our own portable Node.js into this app's own data
    // directory sidesteps that whole class of bug: we know exactly where
    // `node.exe`/`npm.cmd` live, we explicitly control the PATH used for
    // every step of this install, and none of it depends on the Windows
    // registry Environment key or any broadcast/restart at all. The
    // one-time ~30 MB download is a small price for a setup flow that
    // reliably works on the very first try, on any machine.
    let (os, arch, ext) = node_platform_arch()?;
    let folder_name = node_folder_name(os, arch);
    let node_root = tools_root.join("node");
    let extracted_root = node_root.join(&folder_name);
    let bin_dir = node_bin_dir_within(&extracted_root);

    if bin_dir.join(node_exe_name()).exists() {
        emit_progress(app, "extracting_node", "Portable Node.js runtime already present", None);
    } else {
        let url = node_download_url(os, arch, ext);
        emit_progress(
            app,
            "downloading_node",
            &format!("Downloading Node.js {} runtime for {}-{}...", NODE_VERSION, os, arch),
            Some(0.0),
        );

        let archive_path = tools_root.join(format!("node-download.{}", ext));
        download_file_with_progress(app, &url, &archive_path).await?;

        emit_progress(app, "extracting_node", "Extracting Node.js runtime...", None);
        std::fs::create_dir_all(&node_root)
            .map_err(|e| format!("Failed to create Node.js install directory: {}", e))?;

        let archive_for_blocking = archive_path.clone();
        let dest_for_blocking = node_root.clone();
        let ext_owned = ext.to_string();
        tokio::task::spawn_blocking(move || {
            if ext_owned == "zip" {
                extract_zip(&archive_for_blocking, &dest_for_blocking)
            } else {
                extract_tar_xz(&archive_for_blocking, &dest_for_blocking)
            }
        })
        .await
        .map_err(|e| format!("Node.js extraction task panicked: {}", e))??;

        let _ = std::fs::remove_file(&archive_path);
    }

    if !bin_dir.join(node_exe_name()).exists() {
        return Err(
            "Node.js was downloaded and extracted but the expected binaries were not found"
                .to_string(),
        );
    }

    prepend_process_path(&bin_dir);
    let npm_bin_path = bin_dir.join(npm_exe_name());

    if !npm_bin_path.exists() {
        return Err("Node.js was installed but npm was not found in the managed runtime".to_string());
    }

    emit_progress(app, "done", "Node.js and npm are ready", Some(100.0));
    Ok(())
}

/// Installs OpenSpec using only the verified app-managed Node.js/npm runtime.
/// It never uses system npm or system PATH, so a Windows restart is not part
/// of this flow.
pub async fn install_openspec_cli<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    ensure_local_tools_on_path(app);

    if verify_openspec_functional(app).await {
        emit_progress(app, "done", "OpenSpec CLI is already available", Some(100.0));
        save_setup_decision(app, OpenSpecSetupDecision::Installed).await?;
        return Ok(());
    }

    let tools_root = tools_root_dir(app)?;
    let node_bin_dir = discover_installed_node_bin_dir(&tools_root).ok_or_else(|| {
        "Install Node.js and npm from the Desktop Tools settings before installing OpenSpec".to_string()
    })?;
    let npm_bin_path = node_bin_dir.join(npm_exe_name());
    if !npm_bin_path.exists() {
        return Err("The managed Node.js runtime is incomplete: npm was not found".to_string());
    }

    let npm_global_dir = tools_root.join("npm-global");

    // Start from a clean slate: a previous failed attempt can leave behind a
    // partially-installed, locked, or corrupted node_modules tree (this is
    // what produced the `npm warn cleanup ... EPERM: operation not permitted`
    // noise seen on Windows). Layering a new install attempt on top of that
    // is unreliable; wiping it first makes every retry deterministic. Errors
    // here are tolerated (best-effort) since some leftover files may still be
    // locked by antivirus/indexing - `npm install` itself will surface a
    // clear error later if that actually blocks the install.
    if npm_global_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&npm_global_dir) {
            log::warn!(
                "Failed to fully clean previous npm-global install directory (continuing anyway): {}",
                e
            );
        }
    }
    std::fs::create_dir_all(&npm_global_dir)
        .map_err(|e| format!("Failed to create npm global install directory: {}", e))?;

    emit_progress(
        app,
        "installing_openspec",
        "Installing @fission-ai/openspec via npm (this can take a minute)...",
        None,
    );
    run_npm_install_openspec(app, &npm_bin_path, &node_bin_dir, &npm_global_dir).await?;
    run_openspec_postinstall(app, &node_bin_dir, &npm_global_dir).await?;

    ensure_local_tools_on_path(app);

    if !verify_openspec_functional(app).await {
        return Err(
            "npm reported success but the installed OpenSpec CLI does not run correctly. \
             Check the log above for the actual npm/postinstall error."
                .to_string(),
        );
    }

    emit_progress(app, "done", "OpenSpec CLI installed successfully", Some(100.0));
    save_setup_decision(app, OpenSpecSetupDecision::Installed).await?;
    Ok(())
}

/// Backwards-compatible combined entry point for callers from earlier builds.
pub async fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    install_portable_node_runtime(app).await?;
    install_openspec_cli(app).await
}

pub fn node_runtime_status<R: Runtime>(app: &AppHandle<R>) -> NodeRuntimeStatusPayload {
    let managed_bin = tools_root_dir(app)
        .ok()
        .and_then(|root| discover_installed_node_bin_dir(&root));
    let (node_available, npm_available, managed_runtime_available) = match managed_bin {
        Some(bin) => (
            bin.join(node_exe_name()).exists(),
            bin.join(npm_exe_name()).exists(),
            true,
        ),
        None => (false, false, false),
    };

    NodeRuntimeStatusPayload {
        node_available,
        npm_available,
        managed_runtime_available,
        version: NODE_VERSION.to_string(),
    }
}

/// Resolves the exact executable pair owned by Meet4Specs for OpenSpec. The
/// generator must use this rather than the `openspec.cmd` shim: on Windows
/// that shim launches `cmd.exe` and can lose the managed Node.js runtime from
/// its environment, despite a successful installation and verification.
pub fn managed_openspec_paths<R: Runtime>(app: &AppHandle<R>) -> Option<(PathBuf, PathBuf)> {
    let tools_root = tools_root_dir(app).ok()?;
    let node_bin_dir = discover_installed_node_bin_dir(&tools_root)?;
    let node_path = node_bin_dir.join(node_exe_name());
    let entrypoint = tools_root
        .join("npm-global")
        .join("node_modules")
        .join("@fission-ai")
        .join("openspec")
        .join("bin")
        .join("openspec.js");

    (node_path.exists() && entrypoint.exists()).then_some((node_path, entrypoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_bin_dir_matches_platform_layout() {
        let root = PathBuf::from("/tmp/node-v20.18.1-linux-x64");
        let bin_dir = node_bin_dir_within(&root);
        if cfg!(windows) {
            assert_eq!(bin_dir, root);
        } else {
            assert_eq!(bin_dir, root.join("bin"));
        }
    }

    #[test]
    fn npm_global_bin_dir_matches_platform_layout() {
        let prefix = PathBuf::from("/tmp/npm-global");
        let bin_dir = npm_global_bin_dir(&prefix);
        if cfg!(windows) {
            assert_eq!(bin_dir, prefix);
        } else {
            assert_eq!(bin_dir, prefix.join("bin"));
        }
    }

    #[test]
    fn node_download_url_is_well_formed() {
        let url = node_download_url("linux", "x64", "tar.xz");
        assert_eq!(
            url,
            format!(
                "https://nodejs.org/dist/v{v}/node-v{v}-linux-x64.tar.xz",
                v = NODE_VERSION
            )
        );
    }

    #[test]
    fn setup_decision_defaults_to_unresolved() {
        assert_eq!(OpenSpecSetupDecision::default(), OpenSpecSetupDecision::Unresolved);
    }
}
