//! macOS menu-bar app: tray icon, menu, and the event loop that ties detection,
//! recording, and registration together.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use chrono::{DateTime, Utc};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::detection::Signal;
use crate::recorder::Recorder;
use crate::register::{self, Finalize};

/// Recordings shorter than this are treated as false positives (e.g. a short
/// voice message that briefly grabbed the mic) and discarded, not registered.
const MIN_MEETING_SECONDS: f64 = 60.0;

/// State shared between the UI thread and the background detection thread.
pub struct Shared {
    /// Whether auto-detection is active (toggled from the tray menu).
    pub enabled: AtomicBool,
    /// Whether a recording is currently in progress.
    pub recording: AtomicBool,
}

/// A recording currently in progress (lives only on the UI thread).
struct ActiveRecording {
    recorder: Recorder,
    folder: PathBuf,
    title: String,
    meeting_id: String,
    started: DateTime<Utc>,
}

/// Events funnelled into the tao event loop from global handlers / worker threads.
enum UserEvent {
    Menu(MenuEvent),
    #[allow(dead_code)]
    Tray(TrayIconEvent),
    Detection(Signal),
    Finalized {
        title: String,
        registered: bool,
        error: Option<String>,
    },
}

// `tray` is assigned once and then only held alive for the app's lifetime;
// dropping it would remove the menu-bar icon. That read-less "keep-alive" is
// intentional, so quiet the unused-assignment lint for this function.
#[allow(unused_assignments)]
pub fn run() {
    let shared = Arc::new(Shared {
        enabled: AtomicBool::new(true),
        recording: AtomicBool::new(false),
    });

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // Accessory => menu-bar-only app, no Dock icon (even without an Info.plist).
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    // Forward global menu/tray events into the tao loop as user events.
    let proxy = Arc::new(Mutex::new(event_loop.create_proxy()));
    {
        let proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(UserEvent::Menu(event));
            }
        }));
    }
    {
        let proxy = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(UserEvent::Tray(event));
            }
        }));
    }
    // Plain proxies owned by worker threads (single owner each, no Mutex needed).
    let detection_proxy = event_loop.create_proxy();
    let finalize_proxy = event_loop.create_proxy();

    // Menu items are shared handles; keep clones to update text / read state.
    let status_item = MenuItem::new("● Watching for meetings", false, None);
    let pause_item = CheckMenuItem::new("Pause auto-detection", true, false, None);
    let login_item = CheckMenuItem::new("Start at login", true, crate::login_item::is_enabled(), None);
    let open_item = MenuItem::new("Open recordings folder", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &status_item,
        &PredefinedMenuItem::separator(),
        &pause_item,
        &login_item,
        &open_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("build tray menu");

    let pause_id = pause_item.id().clone();
    let login_id = login_item.id().clone();
    let open_id = open_item.id().clone();
    let quit_id = quit_item.id().clone();

    // The tray icon must be created after the app has initialized on macOS.
    let mut tray: Option<TrayIcon> = None;
    let mut menu_holder = Some(menu);
    let mut active: Option<ActiveRecording> = None;

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                let built = TrayIconBuilder::new()
                    .with_menu(Box::new(menu_holder.take().expect("menu built once")))
                    .with_tooltip("Memento — auto meeting recorder")
                    .with_icon(load_icon())
                    // Template image => macOS tints it to match the menu bar (light/dark).
                    .with_icon_as_template(true)
                    .build();
                match built {
                    Ok(t) => tray = Some(t),
                    Err(e) => {
                        log::error!("failed to create tray icon: {e}");
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                // Keep the tray handle alive for the app's lifetime (dropping it
                // removes the menu-bar icon).
                log::debug!("tray active: {}", tray.is_some());
                log::info!("memento-detector started; watching for meetings");

                let emit_proxy = detection_proxy.clone();
                crate::detection::spawn(shared.clone(), move |signal| {
                    let _ = emit_proxy.send_event(UserEvent::Detection(signal));
                });
            }

            Event::UserEvent(UserEvent::Menu(menu_event)) => {
                let id = menu_event.id;
                if id == quit_id {
                    log::info!("quit requested");
                    finalize_on_quit(active.take());
                    *control_flow = ControlFlow::Exit;
                } else if id == open_id {
                    let folder = crate::paths::recordings_folder();
                    let _ = std::fs::create_dir_all(&folder);
                    if let Err(e) = std::process::Command::new("open").arg(&folder).spawn() {
                        log::warn!("could not open recordings folder: {e}");
                    }
                } else if id == pause_id {
                    // muda toggles the check state before delivering the event:
                    // checked == paused.
                    let paused = pause_item.is_checked();
                    shared.enabled.store(!paused, Ordering::SeqCst);
                    log::info!("auto-detection {}", if paused { "paused" } else { "resumed" });
                    update_status(&status_item, &shared);
                } else if id == login_id {
                    let want = login_item.is_checked();
                    let result = if want {
                        crate::login_item::enable()
                    } else {
                        crate::login_item::disable()
                    };
                    if let Err(e) = result {
                        log::error!("could not update start-at-login: {e}");
                        // Revert the checkbox to reflect the real state.
                        login_item.set_checked(crate::login_item::is_enabled());
                    }
                }
            }

            Event::UserEvent(UserEvent::Tray(_)) => {}

            Event::UserEvent(UserEvent::Detection(signal)) => match signal {
                Signal::MeetingStarted { label } => {
                    if active.is_some() {
                        log::debug!("already recording; ignoring duplicate start");
                    } else {
                        match start_recording(&label) {
                            Ok(rec) => {
                                notify("Recording started", &format!("Memento is recording {label}."));
                                active = Some(rec);
                                shared.recording.store(true, Ordering::SeqCst);
                                update_status(&status_item, &shared);
                            }
                            Err(e) => {
                                log::error!("failed to start recording: {e:#}");
                                notify("Recording failed to start", &format!("{e}"));
                            }
                        }
                    }
                }
                Signal::MeetingStopped => {
                    shared.recording.store(false, Ordering::SeqCst);
                    update_status(&status_item, &shared);
                    if let Some(rec) = active.take() {
                        stop_and_finalize(rec, &finalize_proxy);
                    }
                }
            },

            Event::UserEvent(UserEvent::Finalized {
                title,
                registered,
                error,
            }) => match error {
                None if registered => notify(
                    "Recording saved",
                    &format!("“{title}” was added to Memento. Open it and press Enhance to transcribe."),
                ),
                None => notify(
                    "Recording saved",
                    &format!("“{title}” was saved to your recordings folder. Import it in Memento to transcribe."),
                ),
                Some(e) => {
                    log::error!("finalize failed for “{title}”: {e}");
                    notify("Recording error", &format!("Could not save “{title}”: {e}"));
                }
            },

            _ => {}
        }
    });
}

/// Stop the recorder and, if the recording is long enough, finalize it on a
/// worker thread (transcode + register), reporting back via a Finalized event.
fn stop_and_finalize(rec: ActiveRecording, finalize_proxy: &tao::event_loop::EventLoopProxy<UserEvent>) {
    let ActiveRecording {
        recorder,
        folder,
        title,
        meeting_id,
        started,
    } = rec;

    let info = match recorder.stop() {
        Ok(info) => info,
        Err(e) => {
            log::error!("failed to stop recording: {e:#}");
            let _ = std::fs::remove_dir_all(&folder);
            return;
        }
    };

    if info.duration_secs < MIN_MEETING_SECONDS {
        log::info!(
            "discarding short recording ({:.1}s < {:.0}s): {title}",
            info.duration_secs,
            MIN_MEETING_SECONDS
        );
        let _ = std::fs::remove_dir_all(&folder);
        return;
    }

    log::info!("meeting ended; finalizing “{title}” ({:.0}s)", info.duration_secs);
    let input = Finalize {
        meeting_id,
        folder,
        wav_path: info.wav_path,
        title: title.clone(),
        device_name: info.device_name,
        duration_secs: info.duration_secs,
        started,
    };
    let proxy = finalize_proxy.clone();
    std::thread::spawn(move || {
        let event = match register::finalize(input) {
            Ok(outcome) => UserEvent::Finalized {
                title,
                registered: outcome.registered,
                error: None,
            },
            Err(e) => UserEvent::Finalized {
                title,
                registered: false,
                error: Some(e.to_string()),
            },
        };
        let _ = proxy.send_event(event);
    });
}

/// Finalize an in-progress recording synchronously (used on quit).
fn finalize_on_quit(active: Option<ActiveRecording>) {
    let Some(rec) = active else { return };
    let ActiveRecording {
        recorder,
        folder,
        title,
        meeting_id,
        started,
    } = rec;
    let Ok(info) = recorder.stop() else {
        let _ = std::fs::remove_dir_all(&folder);
        return;
    };
    if info.duration_secs < MIN_MEETING_SECONDS {
        let _ = std::fs::remove_dir_all(&folder);
        return;
    }
    log::info!("finalizing in-progress recording before quit: “{title}”");
    let input = Finalize {
        meeting_id,
        folder,
        wav_path: info.wav_path,
        title,
        device_name: info.device_name,
        duration_secs: info.duration_secs,
        started,
    };
    if let Err(e) = register::finalize(input) {
        log::error!("finalize on quit failed: {e:#}");
    }
}

/// Create the meeting folder and start capturing the mic into it.
fn start_recording(label: &str) -> Result<ActiveRecording> {
    let started = Utc::now();
    let title = format!("Auto-recording — {label}");
    let folder = make_meeting_folder(&title, started)?;
    let recorder = Recorder::start(folder.join("audio.wav"))?;
    Ok(ActiveRecording {
        recorder,
        folder,
        title,
        meeting_id: format!("meeting-{}", uuid::Uuid::new_v4()),
        started,
    })
}

/// Build a unique `<title>_<YYYY-MM-DD_HH-MM>/` folder under the recordings dir.
fn make_meeting_folder(title: &str, started: DateTime<Utc>) -> Result<PathBuf> {
    let base = crate::paths::recordings_folder();
    let stamp = started.format("%Y-%m-%d_%H-%M");
    let sanitized = sanitize(title);
    let mut folder = base.join(format!("{sanitized}_{stamp}"));
    let mut counter = 2;
    while folder.exists() {
        folder = base.join(format!("{sanitized}_{stamp}_{counter}"));
        counter += 1;
    }
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn notify(title: &str, body: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
    {
        log::warn!("could not show notification: {e}");
    }
}

/// Update the disabled status line to reflect current state.
fn update_status(status_item: &MenuItem, shared: &Shared) {
    let text = if !shared.enabled.load(Ordering::SeqCst) {
        "❚❚ Auto-detection paused"
    } else if shared.recording.load(Ordering::SeqCst) {
        "● Recording…"
    } else {
        "● Watching for meetings"
    };
    status_item.set_text(text);
}

/// Decode the embedded PNG into a tray icon.
fn load_icon() -> Icon {
    let bytes = include_bytes!("../icons/tray-icon.png");
    let image = image::load_from_memory(bytes)
        .expect("valid tray icon png")
        .to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).expect("tray icon rgba")
}
