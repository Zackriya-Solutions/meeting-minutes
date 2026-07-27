use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
};
use tokio::{
    fs::{self, File},
    io::{AsyncWriteExt, BufWriter},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    sync::{broadcast, Mutex, Notify},
    time::{timeout, Duration},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

const BRIDGE_ADDRESS: &str = "127.0.0.1:8179";
const BRIDGE_TOKEN: &str = "meetily-local-capture-v1-7d4f2c9a6e31b805";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoCaptureMode {
    Off,
    Screen,
    Window,
    BrowserTab,
}

impl Default for VideoCaptureMode {
    fn default() -> Self {
        Self::Off
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoCaptureStatus {
    pub mode: VideoCaptureMode,
    pub bridge_connected: bool,
    pub tab_armed: bool,
    pub tab_title: Option<String>,
    pub recording: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureWindow {
    pub id: u32,
    pub app: String,
    pub title: String,
}

enum ActiveCapture {
    Screen {
        child: Child,
        output_path: PathBuf,
    },
    BrowserTab {
        output_path: PathBuf,
        standalone: bool,
    },
}

struct BridgeState {
    mode: Mutex<VideoCaptureMode>,
    connected: AtomicBool,
    tab_armed: AtomicBool,
    tab_title: Mutex<Option<String>>,
    window_id: Mutex<Option<u32>>,
    active: Mutex<Option<ActiveCapture>>,
    last_saved: Mutex<Option<PathBuf>>,
    writer: Mutex<Option<BufWriter<File>>>,
    commands: broadcast::Sender<String>,
    completed: Notify,
}

static STATE: LazyLock<Arc<BridgeState>> = LazyLock::new(|| {
    let (commands, _) = broadcast::channel(16);
    Arc::new(BridgeState {
        mode: Mutex::new(VideoCaptureMode::Off),
        connected: AtomicBool::new(false),
        tab_armed: AtomicBool::new(false),
        tab_title: Mutex::new(None),
        window_id: Mutex::new(None),
        active: Mutex::new(None),
        last_saved: Mutex::new(None),
        writer: Mutex::new(None),
        commands,
        completed: Notify::new(),
    })
});

pub fn start_bridge() {
    tauri::async_runtime::spawn(async {
        let listener = match TcpListener::bind(BRIDGE_ADDRESS).await {
            Ok(listener) => listener,
            Err(error) => {
                log::error!("Unable to start browser capture bridge: {error}");
                return;
            }
        };
        log::info!("Browser capture bridge listening on {BRIDGE_ADDRESS}");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tauri::async_runtime::spawn(handle_extension(stream));
                }
                Err(error) => log::warn!("Browser capture bridge accept failed: {error}"),
            }
        }
    });
}

async fn handle_extension(stream: TcpStream) {
    let websocket = match accept_async(stream).await {
        Ok(socket) => socket,
        Err(error) => {
            log::warn!("Rejected browser capture bridge connection: {error}");
            return;
        }
    };
    let (mut outgoing, mut incoming) = websocket.split();
    let mut commands = STATE.commands.subscribe();
    let mut authorized = false;

    loop {
        tokio::select! {
            command = commands.recv(), if authorized => {
                match command {
                    Ok(command) => {
                        if outgoing.send(Message::Text(command)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            message = incoming.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if is_authorized_message(&text) {
                            authorized = true;
                            STATE.connected.store(true, Ordering::SeqCst);
                            handle_extension_message(&text).await;
                        } else {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) if authorized => {
                        let mut writer = STATE.writer.lock().await;
                        if let Some(writer) = writer.as_mut() {
                            if let Err(error) = writer.write_all(&bytes).await {
                                log::error!("Failed writing browser video chunk: {error}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    STATE.connected.store(false, Ordering::SeqCst);
    STATE.tab_armed.store(false, Ordering::SeqCst);
    *STATE.tab_title.lock().await = None;
}

fn is_authorized_message(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|message| {
            message
                .get("token")
                .and_then(|value| value.as_str())
                .map(|token| token == BRIDGE_TOKEN)
        })
        .unwrap_or(false)
}

async fn handle_extension_message(text: &str) {
    let Ok(message) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    match message.get("type").and_then(|value| value.as_str()) {
        Some("armed") => {
            STATE.tab_armed.store(true, Ordering::SeqCst);
            *STATE.tab_title.lock().await = message
                .get("title")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
        }
        Some("disarmed") => {
            STATE.tab_armed.store(false, Ordering::SeqCst);
            *STATE.tab_title.lock().await = None;
        }
        Some("complete") => {
            if let Some(mut writer) = STATE.writer.lock().await.take() {
                if let Err(error) = writer.flush().await {
                    log::error!("Failed flushing browser video: {error}");
                }
            }
            let standalone_output = {
                let mut active = STATE.active.lock().await;
                match active.as_ref() {
                    Some(ActiveCapture::BrowserTab {
                        output_path,
                        standalone: true,
                    }) => {
                        let output_path = output_path.clone();
                        active.take();
                        Some(output_path)
                    }
                    _ => None,
                }
            };
            if let Some(output_path) = standalone_output {
                match validate_video(&output_path) {
                    Ok(()) => {
                        *STATE.last_saved.lock().await = Some(output_path.clone());
                        let filename = output_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Meetily tab recording.webm");
                        let _ = STATE.commands.send(
                            serde_json::json!({
                                "type": "saved",
                                "filename": filename,
                            })
                            .to_string(),
                        );
                    }
                    Err(error) => {
                        let _ = fs::remove_file(&output_path).await;
                        let _ = STATE.commands.send(
                            serde_json::json!({
                                "type": "error",
                                "message": error,
                            })
                            .to_string(),
                        );
                    }
                }
            }
            STATE.completed.notify_one();
        }
        Some("manual_start") => {
            if let Err(error) = start_standalone_browser_capture().await {
                let _ = STATE.commands.send(
                    serde_json::json!({
                        "type": "error",
                        "message": error,
                    })
                    .to_string(),
                );
            }
        }
        Some("discard") => {
            match discard_browser_capture().await {
                Ok(()) => {
                    let _ = STATE.commands.send(r#"{"type":"discarded"}"#.to_string());
                }
                Err(error) => {
                    let _ = STATE.commands.send(
                        serde_json::json!({
                            "type": "error",
                            "message": error,
                        })
                        .to_string(),
                    );
                }
            }
            STATE.completed.notify_one();
        }
        Some("error") => {
            log::error!(
                "Browser capture extension error: {}",
                message
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown error")
            );
            STATE.completed.notify_one();
        }
        _ => {}
    }
}

#[tauri::command]
pub async fn set_video_capture_mode(
    mode: VideoCaptureMode,
    window_id: Option<u32>,
) -> Result<(), String> {
    if STATE.active.lock().await.is_some() {
        return Err("Cannot change video source while recording".to_string());
    }
    if mode == VideoCaptureMode::Window && window_id.is_none() {
        return Err("Choose a window to record".to_string());
    }
    *STATE.mode.lock().await = mode;
    *STATE.window_id.lock().await = window_id;
    Ok(())
}

#[tauri::command]
pub async fn get_video_capture_status() -> VideoCaptureStatus {
    VideoCaptureStatus {
        mode: *STATE.mode.lock().await,
        bridge_connected: STATE.connected.load(Ordering::SeqCst),
        tab_armed: STATE.tab_armed.load(Ordering::SeqCst),
        tab_title: STATE.tab_title.lock().await.clone(),
        recording: STATE.active.lock().await.is_some(),
    }
}

#[tauri::command]
pub fn get_meeting_video_path(folder_path: String) -> Option<String> {
    [
        Path::new(&folder_path).join("video.mov"),
        Path::new(&folder_path).join("video.webm"),
    ]
    .into_iter()
    .find(|path| {
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.len() >= 1024)
            .unwrap_or(false)
    })
    .map(|path| path.to_string_lossy().to_string())
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn list_capture_windows() -> Vec<CaptureWindow> {
    use core_foundation::{base::TCFType, number::CFNumber, string::CFString};
    use core_graphics::window::{
        create_description_from_array, create_window_list, kCGNullWindowID, kCGWindowLayer,
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowName,
        kCGWindowNumber, kCGWindowOwnerName,
    };

    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let Some(ids) = create_window_list(options, kCGNullWindowID) else {
        return Vec::new();
    };
    let Some(descriptions) = create_description_from_array(ids) else {
        return Vec::new();
    };

    let number_key = unsafe { CFString::wrap_under_get_rule(kCGWindowNumber) };
    let layer_key = unsafe { CFString::wrap_under_get_rule(kCGWindowLayer) };
    let owner_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerName) };
    let name_key = unsafe { CFString::wrap_under_get_rule(kCGWindowName) };

    descriptions
        .iter()
        .filter_map(|description| {
            let layer = description
                .find(&layer_key)?
                .downcast::<CFNumber>()?
                .to_i32()?;
            if layer != 0 {
                return None;
            }
            let id = description
                .find(&number_key)?
                .downcast::<CFNumber>()?
                .to_i64()? as u32;
            let app = description
                .find(&owner_key)?
                .downcast::<CFString>()?
                .to_string();
            let title = description
                .find(&name_key)
                .and_then(|value| value.downcast::<CFString>())
                .map(|value| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| app.clone());
            Some(CaptureWindow { id, app, title })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn list_capture_windows() -> Vec<CaptureWindow> {
    Vec::new()
}

pub async fn start_for_meeting(meeting_folder: &Path) -> Result<(), String> {
    match *STATE.mode.lock().await {
        VideoCaptureMode::Off => Ok(()),
        VideoCaptureMode::Screen => start_screen_capture(meeting_folder).await,
        VideoCaptureMode::Window => start_window_capture(meeting_folder).await,
        VideoCaptureMode::BrowserTab => start_browser_capture(meeting_folder).await,
    }
}

async fn start_screen_capture(meeting_folder: &Path) -> Result<(), String> {
    let output_path = meeting_folder.join("video.mov");
    let child = Command::new("/usr/sbin/screencapture")
        .args(["-v", "-U", "-J", "video", "-x"])
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to open the macOS screen picker: {error}"))?;
    *STATE.active.lock().await = Some(ActiveCapture::Screen { child, output_path });
    Ok(())
}

async fn start_window_capture(meeting_folder: &Path) -> Result<(), String> {
    let window_id =
        (*STATE.window_id.lock().await).ok_or_else(|| "Choose a window to record".to_string())?;
    let output_path = meeting_folder.join("video.mov");
    let child = Command::new("/usr/sbin/screencapture")
        .args(["-v", "-l", &window_id.to_string(), "-x"])
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to record the selected window: {error}"))?;
    *STATE.active.lock().await = Some(ActiveCapture::Screen { child, output_path });
    Ok(())
}

async fn start_browser_capture(meeting_folder: &Path) -> Result<(), String> {
    if STATE.active.lock().await.is_some() {
        return Err("Stop the current tab recording before starting a meeting".to_string());
    }
    if !STATE.connected.load(Ordering::SeqCst) {
        return Err("Browser capture extension is not connected".to_string());
    }
    if !STATE.tab_armed.load(Ordering::SeqCst) {
        return Err("Choose a tab by clicking the Meetily Capture extension first".to_string());
    }

    let output_path = meeting_folder.join("video.webm");
    let file = File::create(&output_path)
        .await
        .map_err(|error| format!("Failed creating browser video: {error}"))?;
    *STATE.writer.lock().await = Some(BufWriter::new(file));
    *STATE.active.lock().await = Some(ActiveCapture::BrowserTab {
        output_path: output_path.clone(),
        standalone: false,
    });
    STATE
        .commands
        .send(r#"{"type":"start","origin":"meeting"}"#.to_string())
        .map_err(|_| {
            "Browser capture extension disconnected before recording started".to_string()
        })?;
    Ok(())
}

async fn start_standalone_browser_capture() -> Result<(), String> {
    if STATE.active.lock().await.is_some() {
        return Err("A Meetily video capture is already active".to_string());
    }
    if !STATE.connected.load(Ordering::SeqCst) {
        return Err("Meetily is not running".to_string());
    }
    if !STATE.tab_armed.load(Ordering::SeqCst) {
        return Err("Select a tab before starting".to_string());
    }

    let capture_dir = dirs::video_dir()
        .or_else(dirs::document_dir)
        .ok_or_else(|| "Could not locate a local video folder".to_string())?
        .join("Meetily Captures");
    fs::create_dir_all(&capture_dir)
        .await
        .map_err(|error| format!("Failed creating Meetily Captures: {error}"))?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("System clock error: {error}"))?
        .as_secs();
    let output_path = capture_dir.join(format!("meetily-tab-{timestamp}.webm"));
    let file = File::create(&output_path)
        .await
        .map_err(|error| format!("Failed creating browser video: {error}"))?;

    *STATE.writer.lock().await = Some(BufWriter::new(file));
    *STATE.active.lock().await = Some(ActiveCapture::BrowserTab {
        output_path,
        standalone: true,
    });
    STATE
        .commands
        .send(r#"{"type":"start","origin":"standalone"}"#.to_string())
        .map_err(|_| {
            "Browser capture extension disconnected before recording started".to_string()
        })?;
    Ok(())
}

async fn discard_browser_capture() -> Result<(), String> {
    if let Some(mut writer) = STATE.writer.lock().await.take() {
        writer
            .flush()
            .await
            .map_err(|error| format!("Failed closing browser video: {error}"))?;
    }

    let output_path = {
        let mut active = STATE.active.lock().await;
        match active.as_ref() {
            Some(ActiveCapture::BrowserTab { output_path, .. }) => {
                let output_path = output_path.clone();
                active.take();
                Some(output_path)
            }
            Some(ActiveCapture::Screen { .. }) => {
                return Err("The extension cannot delete a macOS screen recording".to_string());
            }
            None => STATE.last_saved.lock().await.take(),
        }
    };
    let Some(output_path) = output_path else {
        return Err("There is no tab recording to delete".to_string());
    };
    match fs::remove_file(&output_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed deleting tab recording: {error}")),
    }
}

pub async fn stop_active_capture() -> Result<Option<PathBuf>, String> {
    match STATE.active.lock().await.take() {
        None => Ok(None),
        Some(ActiveCapture::Screen {
            mut child,
            output_path,
        }) => {
            let already_finished = child
                .try_wait()
                .map_err(|error| format!("Failed checking screen capture: {error}"))?
                .is_some();
            if !already_finished {
                let pid = child
                    .id()
                    .ok_or_else(|| "Screen capture process has no process ID".to_string())?;
                let status = Command::new("/bin/kill")
                    .args(["-INT", &pid.to_string()])
                    .status()
                    .await
                    .map_err(|error| format!("Failed stopping screen capture: {error}"))?;
                if !status.success() {
                    return Err("macOS screen capture did not accept the stop signal".to_string());
                }
            }
            timeout(Duration::from_secs(10), child.wait())
                .await
                .map_err(|_| "Timed out finalizing screen video".to_string())?
                .map_err(|error| format!("Failed finalizing screen video: {error}"))?;
            validate_video(&output_path)?;
            Ok(Some(output_path))
        }
        Some(ActiveCapture::BrowserTab { output_path, .. }) => {
            let completed = STATE.completed.notified();
            STATE
                .commands
                .send(r#"{"type":"stop"}"#.to_string())
                .map_err(|_| "Browser capture extension disconnected before stop".to_string())?;
            timeout(Duration::from_secs(10), completed)
                .await
                .map_err(|_| "Timed out finalizing browser video".to_string())?;
            if let Some(mut writer) = STATE.writer.lock().await.take() {
                writer
                    .flush()
                    .await
                    .map_err(|error| format!("Failed flushing browser video: {error}"))?;
            }
            validate_video(&output_path)?;
            Ok(Some(output_path))
        }
    }
}

fn validate_video(path: &Path) -> Result<(), String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("Video was not created: {error}"))?;
    if metadata.len() < 1024 {
        return Err("Video capture ended before any usable frames were saved".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_modes_use_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&VideoCaptureMode::BrowserTab).unwrap(),
            "\"browser_tab\""
        );
        assert_eq!(
            serde_json::from_str::<VideoCaptureMode>("\"screen\"").unwrap(),
            VideoCaptureMode::Screen
        );
    }

    #[test]
    fn rejects_empty_video_files() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(validate_video(file.path()).is_err());
    }

    #[test]
    fn browser_bridge_rejects_messages_without_the_pairing_token() {
        assert!(!is_authorized_message(r#"{"type":"hello"}"#));
        assert!(is_authorized_message(&format!(
            r#"{{"type":"hello","token":"{BRIDGE_TOKEN}"}}"#
        )));
    }
}
