use ffmpeg_sidecar::{
    command::ffmpeg_is_installed,
    download::{check_latest_version, download_ffmpeg_package, ffmpeg_download_url, unpack_ffmpeg},
    paths::sidecar_dir,
    version::ffmpeg_version,
};
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use which::which;

#[cfg(not(windows))]
const EXECUTABLE_NAME: &str = "ffmpeg";

#[cfg(windows)]
const EXECUTABLE_NAME: &str = "ffmpeg.exe";

// Async-safe FFmpeg path storage
static FFMPEG_STATE: once_cell::sync::Lazy<Arc<RwLock<Option<PathBuf>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

/// Check if FFmpeg is ready (non-blocking)
pub fn is_ffmpeg_ready() -> bool {
    // Try to read without blocking
    FFMPEG_STATE.try_read().map(|state| state.is_some()).unwrap_or(false)
}

/// Get FFmpeg path (async, waits for initialization if needed)
pub async fn get_ffmpeg_path() -> Option<PathBuf> {
    let state = FFMPEG_STATE.read().await;
    state.clone()
}

/// Initialize FFmpeg asynchronously (safe to call multiple times)
pub async fn initialize_ffmpeg() -> Result<PathBuf, anyhow::Error> {
    info!("Starting FFmpeg initialization...");

    // Check if already initialized
    {
        let state = FFMPEG_STATE.read().await;
        if let Some(path) = state.as_ref() {
            info!("FFmpeg already initialized at: {:?}", path);
            return Ok(path.clone());
        }
    }

    // Search and initialize in background task
    let path = tokio::task::spawn_blocking(|| find_ffmpeg_path_internal()).await??;

    // Store the result
    {
        let mut state = FFMPEG_STATE.write().await;
        *state = Some(path.clone());
    }

    info!("FFmpeg initialized successfully at: {:?}", path);
    Ok(path)
}

fn find_ffmpeg_path_internal() -> Result<PathBuf, anyhow::Error> {
    debug!("Starting search for ffmpeg executable");

    // Check if `ffmpeg` is in the PATH environment variable
    if let Ok(path) = which(EXECUTABLE_NAME) {
        debug!("Found ffmpeg in PATH: {:?}", path);
        return Ok(path);
    }
    debug!("ffmpeg not found in PATH");

    // Check in $HOME/.local/bin on macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let local_bin = PathBuf::from(home).join(".local").join("bin");
            debug!("Checking $HOME/.local/bin: {:?}", local_bin);
            let ffmpeg_in_local_bin = local_bin.join(EXECUTABLE_NAME);
            if ffmpeg_in_local_bin.exists() {
                debug!("Found ffmpeg in $HOME/.local/bin: {:?}", ffmpeg_in_local_bin);
                return Ok(ffmpeg_in_local_bin);
            }
            debug!("ffmpeg not found in $HOME/.local/bin");
        }
    }

    // Check in current working directory
    if let Ok(cwd) = std::env::current_dir() {
        debug!("Current working directory: {:?}", cwd);
        let ffmpeg_in_cwd = cwd.join(EXECUTABLE_NAME);
        if ffmpeg_in_cwd.is_file() && ffmpeg_in_cwd.exists() {
            debug!(
                "Found ffmpeg in current working directory: {:?}",
                ffmpeg_in_cwd
            );
            return Ok(ffmpeg_in_cwd);
        }
        debug!("ffmpeg not found in current working directory");
    }

    // Check in the same folder as the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_folder) = exe_path.parent() {
            debug!("Executable folder: {:?}", exe_folder);
            let ffmpeg_in_exe_folder = exe_folder.join(EXECUTABLE_NAME);
            if ffmpeg_in_exe_folder.exists() {
                debug!(
                    "Found ffmpeg in executable folder: {:?}",
                    ffmpeg_in_exe_folder
                );
                return Ok(ffmpeg_in_exe_folder);
            }
            debug!("ffmpeg not found in executable folder");

            // Platform-specific checks
            #[cfg(target_os = "macos")]
            {
                let resources_folder = exe_folder.join("../Resources");
                debug!("Resources folder: {:?}", resources_folder);
                let ffmpeg_in_resources = resources_folder.join(EXECUTABLE_NAME);
                if ffmpeg_in_resources.exists() {
                    debug!(
                        "Found ffmpeg in Resources folder: {:?}",
                        ffmpeg_in_resources
                    );
                    return Ok(ffmpeg_in_resources);
                }
                debug!("ffmpeg not found in Resources folder");
            }

            #[cfg(target_os = "linux")]
            {
                let lib_folder = exe_folder.join("lib");
                debug!("Lib folder: {:?}", lib_folder);
                let ffmpeg_in_lib = lib_folder.join(EXECUTABLE_NAME);
                if ffmpeg_in_lib.exists() {
                    debug!("Found ffmpeg in lib folder: {:?}", ffmpeg_in_lib);
                    return Ok(ffmpeg_in_lib);
                }
                debug!("ffmpeg not found in lib folder");
            }
        }
    }

    info!("FFmpeg not found locally. Downloading...");

    // Download and install FFmpeg
    handle_ffmpeg_installation()?;

    // Try to find it again after installation
    if let Ok(path) = which(EXECUTABLE_NAME) {
        debug!("Found ffmpeg after installation: {:?}", path);
        return Ok(path);
    }

    let installation_dir = sidecar_dir()?;
    let ffmpeg_in_installation = installation_dir.join(EXECUTABLE_NAME);
    if ffmpeg_in_installation.is_file() {
        debug!("Found ffmpeg in installation directory: {:?}", ffmpeg_in_installation);
        return Ok(ffmpeg_in_installation);
    }

    error!("FFmpeg not found even after installation attempt");
    Err(anyhow::anyhow!("FFmpeg could not be found or installed. Please install FFmpeg manually."))
}

fn handle_ffmpeg_installation() -> Result<(), anyhow::Error> {
    if ffmpeg_is_installed() {
        debug!("ffmpeg is already installed");
        return Ok(());
    }

    debug!("ffmpeg not found. installing...");
    match check_latest_version() {
        Ok(version) => debug!("latest version: {}", version),
        Err(e) => debug!("skipping version check due to error: {e}"),
    }

    let download_url = ffmpeg_download_url()?;
    let destination = get_ffmpeg_install_dir()?;

    debug!("downloading from: {:?}", download_url);
    let archive_path = download_ffmpeg_package(download_url, &destination)?;
    debug!("downloaded package: {:?}", archive_path);

    debug!("extracting...");
    unpack_ffmpeg(&archive_path, &destination)?;

    let version = ffmpeg_version()?;

    debug!("done! installed ffmpeg version {}", version);
    Ok(())
}

#[cfg(target_os = "macos")]
fn get_ffmpeg_install_dir() -> Result<PathBuf, anyhow::Error> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("couldn't find home directory"))?;

    let local_bin = home.join(".local").join("bin");

    // Create directory if it doesn't exist
    if !local_bin.exists() {
        debug!("creating .local/bin directory");
        std::fs::create_dir_all(&local_bin)?;

        // Check both .bashrc and .zshrc
        let shell_configs = vec![
            home.join(".bashrc"),
            home.join(".bash_profile"), // macOS often uses .bash_profile instead of .bashrc
            home.join(".zshrc"),
        ];

        for config in shell_configs {
            if config.exists() {
                let content = std::fs::read_to_string(&config)?;
                if !content.contains(".local/bin") {
                    debug!("adding .local/bin to PATH in {:?}", config);
                    std::fs::write(
                        config,
                        format!("{}\nexport PATH=\"$HOME/.local/bin:$PATH\"\n", content),
                    )?;
                }
            }
        }
    }

    Ok(local_bin)
}

// For other platforms, keep your existing installation directory logic
#[cfg(not(target_os = "macos"))]
fn get_ffmpeg_install_dir() -> Result<PathBuf, anyhow::Error> {
    // Your existing logic for other platforms
    sidecar_dir().map_err(|e| anyhow::anyhow!(e))
}
