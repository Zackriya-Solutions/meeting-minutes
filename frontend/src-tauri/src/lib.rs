use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;
// Removed unused import

// Performance optimization: Conditional logging macros for hot paths
#[cfg(debug_assertions)]
macro_rules! perf_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(not(debug_assertions))]
macro_rules! perf_debug {
    ($($arg:tt)*) => {};
}

#[cfg(debug_assertions)]
macro_rules! perf_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(not(debug_assertions))]
macro_rules! perf_trace {
    ($($arg:tt)*) => {};
}

// Make these macros available to other modules
#[allow(unused_imports)]
pub(crate) use perf_debug;
#[allow(unused_imports)]
pub(crate) use perf_trace;

// Re-export async logging macros for external use (removed due to macro conflicts)

// Declare audio module
pub mod analytics;
pub mod anthropic;
pub mod api;
/// The macOS application menu bar (Check for Updates, Settings, standard edit commands).
#[cfg(target_os = "macos")]
pub mod app_menu;
pub mod audio;
pub mod background_capture;
pub mod calendar;
pub mod collections;
pub mod config;
pub mod console_utils;
pub mod database;
pub mod demo_meeting;
pub mod gateway_identity;
pub mod gigaam_engine;
pub mod groq;
pub mod jobs;
pub mod learning;
pub mod llm;
pub mod meeting_detection;
pub mod notifications;
pub mod ollama;
pub mod onboarding;
pub mod openai;
pub mod openrouter;
pub mod parakeet_engine;
pub mod pipeline;
pub mod report;
pub mod salutespeech;
pub mod search;
pub mod state;
pub mod summary;
pub mod telegram;
pub mod tray;
pub mod utils;
pub mod vector;
pub mod whisper_engine;
pub mod window_motion;

use audio::{list_audio_devices, trigger_audio_permission, AudioDevice};
use log::{error as log_error, info as log_info};
use notifications::commands::NotificationManagerState;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::RwLock;

static RECORDING_FLAG: AtomicBool = AtomicBool::new(false);

// Global language preference storage (default to "auto-translate" for automatic translation to English)
static LANGUAGE_PREFERENCE: std::sync::LazyLock<StdMutex<String>> =
    std::sync::LazyLock::new(|| StdMutex::new("auto-translate".to_string()));

#[derive(Debug, Deserialize)]
struct RecordingArgs {
    save_path: String,
}

#[derive(Debug, Serialize, Clone)]
struct TranscriptionStatus {
    chunks_in_queue: usize,
    is_processing: bool,
    last_activity_ms: u64,
}

#[tauri::command]
async fn start_recording<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
    meeting_name: Option<String>,
) -> Result<(), String> {
    log_info!("🔥 CALLED start_recording with meeting: {:?}", meeting_name);
    log_info!(
        "📋 Backend received parameters - mic: {:?}, system: {:?}, meeting: {:?}",
        mic_device_name,
        system_device_name,
        meeting_name
    );

    if is_recording().await {
        return Err("Recording already in progress".to_string());
    }

    // Call the actual audio recording system with meeting name
    match audio::recording_commands::start_recording_with_devices_and_meeting(
        app.clone(),
        mic_device_name,
        system_device_name,
        meeting_name.clone(),
    )
    .await
    {
        Ok(_) => {
            RECORDING_FLAG.store(true, Ordering::SeqCst);
            tray::update_tray_menu(&app);

            log_info!("Recording started successfully");

            Ok(())
        }
        Err(e) => {
            log_error!("Failed to start audio recording: {}", e);
            Err(format!("Failed to start recording: {}", e))
        }
    }
}

#[tauri::command]
async fn stop_recording<R: Runtime>(
    app: AppHandle<R>,
    args: RecordingArgs,
) -> Result<audio::recording_commands::StopRecordingOutcome, String> {
    log_info!("Attempting to stop recording...");

    // Check the actual audio recording system state instead of the flag
    if !audio::recording_commands::is_recording().await {
        log_info!("Recording is already stopped");
        RECORDING_FLAG.store(false, Ordering::SeqCst);
        tray::update_tray_menu(&app);
        return Ok(audio::recording_commands::StopRecordingOutcome {
            status: "success".to_string(),
            message: "Recording was already stopped".to_string(),
            stop_error: None,
        });
    }

    // Call the actual audio recording system to stop
    match audio::recording_commands::stop_recording(
        app.clone(),
        audio::recording_commands::RecordingArgs {
            save_path: args.save_path.clone(),
        },
    )
    .await
    {
        Ok(outcome) => {
            RECORDING_FLAG.store(false, Ordering::SeqCst);
            tray::update_tray_menu(&app);

            // Create the save directory if it doesn't exist
            if let Some(parent) = std::path::Path::new(&args.save_path).parent() {
                if !parent.exists() {
                    log_info!("Creating directory: {:?}", parent);
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        let err_msg = format!("Failed to create save directory: {}", e);
                        log_error!("{}", err_msg);
                        return Err(err_msg);
                    }
                }
            }

            Ok(outcome)
        }
        Err(e) => {
            log_error!("Failed to stop audio recording: {}", e);
            // Still update the flag even if stopping failed
            RECORDING_FLAG.store(false, Ordering::SeqCst);
            tray::update_tray_menu(&app);
            Err(format!("Failed to stop recording: {}", e))
        }
    }
}

#[tauri::command]
async fn is_recording() -> bool {
    audio::recording_commands::is_recording().await
}

#[tauri::command]
fn get_transcription_status() -> TranscriptionStatus {
    TranscriptionStatus {
        chunks_in_queue: 0,
        is_processing: false,
        last_activity_ms: 0,
    }
}

#[tauri::command]
async fn save_transcript(file_path: String, content: String) -> Result<(), String> {
    log_info!("Saving transcript to: {}", file_path);

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    // Write content to file
    std::fs::write(&file_path, content)
        .map_err(|e| format!("Failed to write transcript: {}", e))?;

    log_info!("Transcript saved successfully");
    Ok(())
}

// Audio level monitoring commands
#[tauri::command]
async fn start_audio_level_monitoring<R: Runtime>(
    app: AppHandle<R>,
    device_names: Vec<String>,
) -> Result<(), String> {
    log_info!(
        "Starting audio level monitoring for devices: {:?}",
        device_names
    );

    audio::simple_level_monitor::start_monitoring(app, device_names)
        .await
        .map_err(|e| format!("Failed to start audio level monitoring: {}", e))
}

#[tauri::command]
async fn stop_audio_level_monitoring() -> Result<(), String> {
    log_info!("Stopping audio level monitoring");

    audio::simple_level_monitor::stop_monitoring()
        .await
        .map_err(|e| format!("Failed to stop audio level monitoring: {}", e))
}

#[tauri::command]
async fn is_audio_level_monitoring() -> bool {
    audio::simple_level_monitor::is_monitoring()
}

#[tauri::command]
fn get_current_microphone_level() -> f32 {
    audio::pipeline::current_microphone_level()
}

// Analytics commands are now handled by analytics::commands module

// Whisper commands are now handled by whisper_engine::commands module

#[tauri::command]
async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    list_audio_devices()
        .await
        .map_err(|e| format!("Failed to list audio devices: {}", e))
}

#[tauri::command]
async fn trigger_microphone_permission() -> Result<bool, String> {
    trigger_audio_permission()
        .map_err(|e| format!("Failed to trigger microphone permission: {}", e))
}

#[tauri::command]
async fn start_recording_with_devices<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
) -> Result<(), String> {
    start_recording_with_devices_and_meeting(app, mic_device_name, system_device_name, None).await
}

#[tauri::command]
async fn start_recording_with_devices_and_meeting<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
    meeting_name: Option<String>,
) -> Result<(), String> {
    log_info!("🚀 CALLED start_recording_with_devices_and_meeting - Mic: {:?}, System: {:?}, Meeting: {:?}",
             mic_device_name, system_device_name, meeting_name);

    // Call the recording module functions that support meeting names
    let recording_result = match (mic_device_name.clone(), system_device_name.clone()) {
        (None, None) => {
            log_info!(
                "No devices specified, starting with defaults and meeting: {:?}",
                meeting_name
            );
            audio::recording_commands::start_recording_with_meeting_name(app.clone(), meeting_name)
                .await
        }
        _ => {
            log_info!(
                "Starting with specified devices: mic={:?}, system={:?}, meeting={:?}",
                mic_device_name,
                system_device_name,
                meeting_name
            );
            audio::recording_commands::start_recording_with_devices_and_meeting(
                app.clone(),
                mic_device_name,
                system_device_name,
                meeting_name,
            )
            .await
        }
    };

    match recording_result {
        Ok(_) => {
            log_info!("Recording started successfully via tauri command");

            Ok(())
        }
        Err(e) => {
            log_error!("Failed to start recording via tauri command: {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
async fn set_language_preference(language: String) -> Result<(), String> {
    let mut lang_pref = LANGUAGE_PREFERENCE
        .lock()
        .map_err(|e| format!("Failed to set language preference: {}", e))?;
    log_info!("Setting language preference to: {}", language);
    *lang_pref = language;
    Ok(())
}

// Internal helper function to get language preference (for use within Rust code)
pub fn get_language_preference_internal() -> Option<String> {
    LANGUAGE_PREFERENCE.lock().ok().map(|lang| lang.clone())
}

pub fn run() {
    log::set_max_level(log::LevelFilter::Info);

    let mut builder = tauri::Builder::default();

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            log_info!(
                "Second app instance requested with args: {:?}, cwd: {:?}",
                args,
                cwd
            );

            tray::focus_main_window(app);
        }));
    }

    // Menu-bar selections arrive here, not through the tray's own handler, so they are
    // routed to the same place (see `app_menu`).
    #[cfg(target_os = "macos")]
    {
        builder = builder.on_menu_event(|app, event| {
            tray::handle_menu_event(app, event.id.as_ref());
        });
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(whisper_engine::parallel_commands::ParallelProcessorState::new())
        .manage(Arc::new(RwLock::new(
            None::<notifications::manager::NotificationManager<tauri::Wry>>,
        )) as NotificationManagerState<tauri::Wry>)
        .manage(meeting_detection::AutoMeetingDetectionState::default())
        .manage(background_capture::BackgroundCaptureState::default())
        .manage(audio::init_system_audio_state())
        .manage(summary::summary_engine::ModelManagerState(Arc::new(tokio::sync::Mutex::new(None))))
        .setup(|_app| {
            log::info!("Application setup complete");
            let corpus_mode = summary::corpus_runner::corpus_mode_requested();

            if corpus_mode {
                log::info!("Starting isolated standup corpus mode");
            }

            if !corpus_mode {
                // Initialize system tray
                if let Err(e) = tray::create_tray(_app.handle()) {
                    log::error!("Failed to create system tray: {}", e);
                }

                // macOS menu bar. Non-fatal: a missing menu bar is a degraded UI, not a
                // reason to refuse to start.
                #[cfg(target_os = "macos")]
                if let Err(e) = app_menu::install(_app.handle()) {
                    log::error!("Failed to install the application menu: {}", e);
                }

                // Initialize notification system with proper defaults
                log::info!("Initializing notification system...");
                let app_for_notif = _app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let notif_state = app_for_notif.state::<NotificationManagerState<tauri::Wry>>();
                    match notifications::commands::initialize_notification_manager(app_for_notif.clone()).await {
                        Ok(manager) => {
                            // Set default consent and permissions on first launch
                            if let Err(e) = manager.set_consent(true).await {
                                log::error!("Failed to set initial consent: {}", e);
                            }
                            if let Err(e) = manager.request_permission().await {
                                log::error!("Failed to request initial permission: {}", e);
                            }

                            // Store the initialized manager
                            let mut state_lock = notif_state.write().await;
                            *state_lock = Some(manager);
                            log::info!("Notification system initialized with default permissions");
                        }
                        Err(e) => {
                            log::error!("Failed to initialize notification manager: {}", e);
                        }
                    }
                });

                // Set models directory to use app_data_dir (unified storage location)
                whisper_engine::commands::set_models_directory(&_app.handle());

                // Initialize Whisper engine on startup
                tauri::async_runtime::spawn(async {
                    if let Err(e) = whisper_engine::commands::whisper_init().await {
                        log::error!("Failed to initialize Whisper engine on startup: {}", e);
                    }
                });

                // Start the privacy-preserving process/microphone signal detector. On platforms
                // with a strong microphone-session signal it can request the normal recording
                // lifecycle for supported native meeting clients.
                _app
                    .state::<meeting_detection::AutoMeetingDetectionState>()
                    .start(_app.handle().clone());

                // Set Parakeet models directory
                parakeet_engine::commands::set_models_directory(&_app.handle());

                // Initialize Parakeet engine on startup
                tauri::async_runtime::spawn(async {
                    if let Err(e) = parakeet_engine::commands::parakeet_init().await {
                        log::error!("Failed to initialize Parakeet engine on startup: {}", e);
                    }
                });
            }

            // Initialize ModelManager for summary engine (async, non-blocking)
            let app_handle_for_model_manager = _app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match summary::summary_engine::commands::init_model_manager_at_startup(&app_handle_for_model_manager).await {
                    Ok(_) => log::info!("ModelManager initialized successfully at startup"),
                    Err(e) => {
                        log::warn!("Failed to initialize ModelManager at startup: {}", e);
                        log::warn!("ModelManager will be lazy-initialized on first use");
                    }
                }
            });

            // Trigger system audio permission request on startup (similar to microphone permission)
            // #[cfg(target_os = "macos")]
            // {
            //     tauri::async_runtime::spawn(async {
            //         if let Err(e) = audio::permissions::trigger_system_audio_permission() {
            //             log::warn!("Failed to trigger system audio permission: {}", e);
            //         }
            //     });
            // }

            // Store the process-wide app handle for the background job runner (which owns
            // no AppHandle). Must precede DB init, since that spawns the job runner.
            if !corpus_mode {
                pipeline::diarization_commands::set_app_handle(_app.handle().clone());
            }

            // Initialize database (handles first launch detection and conditional setup)
            tauri::async_runtime::block_on(async {
                database::setup::initialize_database_on_startup(
                    &_app.handle(),
                    !corpus_mode,
                    !corpus_mode,
                )
                .await
            })
            .expect("Failed to initialize database");

            if let Some(state) = _app.try_state::<state::AppState>() {
                let pool = state.db_manager.pool().clone();
                tauri::async_runtime::spawn(async move {
                    match summary::interview_workflow::purge_expired(&pool).await {
                        Ok(ids) if !ids.is_empty() => {
                            log::info!("Purged {} expired Interview Memory item(s)", ids.len())
                        }
                        Ok(_) => {}
                        Err(error) => log::warn!("Could not enforce Interview Memory retention: {error}"),
                    }
                    match meeting_detection::purge_expired_capture_data(&pool).await {
                        Ok(count) if count > 0 => {
                            log::info!("Discarded {count} expired unpromoted capture session(s)")
                        }
                        Ok(_) => {}
                        Err(error) => log::warn!("Could not enforce capture metadata retention: {error}"),
                    }
                    match meeting_detection::purge_expired_saved_captures(&pool).await {
                        Ok(ids) if !ids.is_empty() => {
                            log::info!("Purged {} expired auto-captured meeting(s)", ids.len())
                        }
                        Ok(_) => {}
                        Err(error) => log::warn!("Could not enforce auto-capture audio retention: {error}"),
                    }
                    // Reports parked mid-run (e.g. at the clarify/speaker prompt) cannot
                    // survive a restart — their pipeline lives only in memory — so mark
                    // any orphaned rows failed to make the meeting restartable.
                    report::pipeline::recover_interrupted_reports(&pool).await;
                });
            }

            if !corpus_mode {
                // Load the local embedding model in the background if it's already downloaded
                // (enables the vector branch of search/RAG). Never blocks startup.
                {
                    let app_handle = _app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        pipeline::commands::init_embedder_at_startup(&app_handle).await;
                    });
                }

                // Load the GigaAM transcription model in the background if downloaded.
                {
                    let app_handle = _app.handle().clone();
                    #[cfg(debug_assertions)]
                    let batch_folder = std::env::var_os("MEETILY_BATCH_IMPORT_FOLDER")
                        .map(std::path::PathBuf::from);
                    #[cfg(debug_assertions)]
                    let batch_provider = std::env::var("MEETILY_BATCH_IMPORT_PROVIDER").ok();
                    #[cfg(debug_assertions)]
                    let batch_model = std::env::var("MEETILY_BATCH_IMPORT_MODEL").ok();
                    #[cfg(debug_assertions)]
                    let batch_language = std::env::var("MEETILY_BATCH_IMPORT_LANGUAGE").ok();
                    #[cfg(debug_assertions)]
                    let batch_report = std::env::var_os("MEETILY_BATCH_IMPORT_REPORT")
                        .map(std::path::PathBuf::from);
                    tauri::async_runtime::spawn(async move {
                        gigaam_engine::commands::init_gigaam_at_startup(&app_handle).await;
                        if let Some(state) = app_handle.try_state::<state::AppState>() {
                            match jobs::enqueue_missing_transcript_refinement(
                                state.db_manager.pool(),
                            )
                            .await
                            {
                                Ok(0) => {}
                                Ok(count) => log::info!(
                                    "Queued {count} completed recording transcript repair job(s)"
                                ),
                                Err(error) => log::warn!(
                                    "Could not queue completed recording transcript repairs: {error}"
                                ),
                            }
                        }
                        #[cfg(debug_assertions)]
                        if let Some(folder) = batch_folder {
                            log::info!(
                                "Starting configured resumable batch import from {}",
                                folder.display()
                            );
                            match audio::import::start_batch_import_folder(
                                app_handle,
                                folder,
                                batch_language,
                                batch_model,
                                batch_provider,
                                false,
                                batch_report,
                            )
                            .await
                            {
                                Ok(result) => log::info!(
                                    "Configured batch import complete: {} imported, {} skipped, {} failed",
                                    result.imported.len(),
                                    result.skipped.len(),
                                    result.failed.len()
                                ),
                                Err(error) => {
                                    log::error!("Configured batch import failed: {}", error)
                                }
                            }
                        }
                    });
                }
            }

            // Initialize bundled templates directory for dynamic template discovery
            log::info!("Initializing bundled templates directory...");
            if let Ok(resource_path) = _app.handle().path().resource_dir() {
                let templates_dir = resource_path.join("templates");
                log::info!("Setting bundled templates directory to: {:?}", templates_dir);
                summary::templates::set_bundled_templates_dir(templates_dir);
            } else {
                log::warn!("Failed to resolve resource directory for templates");
            }

            // Recover summaries that older builds never started. The automatic version
            // marker prevents a failed provider from being retried on every launch.
            if !corpus_mode {
                let app_handle = _app.handle().clone();
                if let Some(state) = _app.try_state::<state::AppState>() {
                    let pool = state.db_manager.pool().clone();
                    let launched_at = chrono::Utc::now();
                    tauri::async_runtime::spawn(async move {
                        // A generation cannot survive an app exit, so anything still marked
                        // pending here is dead. Clear it first: otherwise the backfill below
                        // skips the meeting as "already running" and the meeting screen keeps
                        // polling that pending status with a spinner that never resolves.
                        match summary::commands::recover_interrupted_summaries(&pool, launched_at)
                            .await
                        {
                            Ok(recovered) if recovered.is_empty() => {}
                            Ok(recovered) => log::info!(
                                "Recovered {} interrupted meeting summary generation(s)",
                                recovered.len()
                            ),
                            Err(error) => {
                                log::warn!("Could not recover interrupted summaries: {error}")
                            }
                        }
                        // Before naming anything new, take back the names an earlier build
                        // applied from evidence today's gate rejects.
                        match pipeline::speaker_names::repair_implausible_automatic_names(&pool)
                            .await
                        {
                            Ok(0) => {}
                            Ok(repaired) => log::info!(
                                "Took back {repaired} implausible automatic speaker name(s)"
                            ),
                            Err(error) => {
                                log::warn!("Could not repair automatic speaker names: {error}")
                            }
                        }
                        match pipeline::speaker_names::backfill_existing_speaker_names(&pool).await {
                            Ok((checked, 0)) => log::info!(
                                "Checked automatic speaker names for {checked} diarized meeting(s)"
                            ),
                            Ok((checked, applied)) => log::info!(
                                "Checked {checked} diarized meeting(s) and applied {applied} automatic speaker name(s)"
                            ),
                            Err(error) => {
                                log::warn!("Could not backfill automatic speaker names: {error}")
                            }
                        }
                        match summary::commands::backfill_missing_automatic_summaries(
                            app_handle,
                            pool,
                        )
                        .await
                        {
                            Ok(0) => {}
                            Ok(count) => {
                                log::info!("Started {count} missing automatic meeting summary job(s)")
                            }
                            Err(error) => {
                                log::warn!("Could not backfill automatic meeting summaries: {error}")
                            }
                        }
                    });
                }
            }

            // Explicit local corpus automation. It is inert unless meeting IDs are supplied;
            // ordinary users never trigger evaluation work at startup.
            if let Ok(raw_ids) = std::env::var("MEETILY_STANDUP_CORPUS_IDS") {
                let meeting_ids = raw_ids
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if !meeting_ids.is_empty() {
                    let provider = std::env::var("MEETILY_STANDUP_CORPUS_PROVIDER")
                        .unwrap_or_else(|_| "builtin-ai".to_string());
                    let model = std::env::var("MEETILY_STANDUP_CORPUS_MODEL")
                        .unwrap_or_else(|_| "qwen3.5:4b".to_string());
                    let summary_language =
                        std::env::var("MEETILY_STANDUP_CORPUS_LANGUAGE").ok();
                    let overwrite = summary::corpus_runner::corpus_overwrite_requested();
                    let report_path = std::env::var_os("MEETILY_STANDUP_CORPUS_REPORT")
                        .map(std::path::PathBuf::from);
                    let app_handle = _app.handle().clone();
                    if let Some(state) = _app.try_state::<state::AppState>() {
                        let pool = state.db_manager.pool().clone();
                        tauri::async_runtime::spawn(async move {
                            match summary::corpus_runner::run_standup_corpus(
                                app_handle,
                                pool,
                                meeting_ids,
                                provider,
                                model,
                                summary_language,
                                overwrite,
                                report_path,
                            )
                            .await
                            {
                                Ok(report) => log::info!(
                                    "Standup corpus run complete: {} completed, {} skipped, {} declined, {} failed",
                                    report.completed,
                                    report.skipped,
                                    report.declined,
                                    report.failed
                                ),
                                Err(error) => log::error!("Standup corpus run failed: {error}"),
                            }
                        });
                    } else {
                        log::error!(
                            "Standup corpus run cannot start before the local database is initialized"
                        );
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main"
                    && !summary::corpus_runner::corpus_mode_requested()
                {
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        log::error!("Failed to hide main window on close request: {}", e);
                    } else {
                        log::info!("Main window hidden to tray on close request");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            search::commands::search_meetings,
            search::commands::rag_ask,
            search::commands::rag_get_latest_session,
            pipeline::commands::embedder_status,
            pipeline::commands::indexing_status,
            pipeline::commands::embedder_download_model,
            pipeline::commands::embedder_select_model,
            pipeline::diarization_commands::diarization_status,
            pipeline::diarization_commands::download_diarization_models,
            pipeline::diarization_commands::diarize_meeting,
            audio::refinement::rerun_meeting_refinement,
            pipeline::diarization_commands::get_meeting_speakers,
            pipeline::diarization_commands::rename_speaker,
            pipeline::diarization_commands::assign_segment_speaker,
            pipeline::diarization_commands::add_and_assign_segment_speaker,
            pipeline::diarization_commands::set_self_speaker,
            pipeline::diarization_commands::set_meeting_diarization_prefs,
            learning::identity::get_identity_review,
            learning::identity::review_speaker_identity,
            learning::identity::list_speaker_profile_versions,
            learning::identity::rollback_speaker_profile,
            learning::identity::purge_speaker_learning_data,
            learning::identity::learn_voices_from_past_meetings,
            learning::advanced::get_speaker_advanced_learning,
            learning::advanced::set_speaker_advanced_learning,
            learning::classification::classify_meeting,
            learning::classification::get_meeting_classification_review,
            learning::classification::get_collection_classification_review,
            learning::classification::list_learning_inbox,
            learning::classification::review_meeting_classification,
            learning::classification::review_collection_classification,
            learning::terminology::correct_transcript_segment,
            learning::terminology::list_terminology_memory,
            learning::terminology::review_terminology_memory,
            learning::reconciliation::list_reconciliation_suggestions,
            learning::reconciliation::review_reconciliation_suggestion,
            learning::reconciliation::rollback_reconciliation_suggestion,
            pipeline::speaker_names::scan_speaker_name_candidates,
            pipeline::speaker_names::infer_meeting_speaker_names,
            pipeline::speaker_names::list_speaker_name_candidates,
            pipeline::speaker_names::review_speaker_name_candidate,
            gigaam_engine::commands::gigaam_status,
            gigaam_engine::commands::gigaam_download_model,
            gigaam_engine::commands::gigaam_select_variant,
            gigaam_engine::commands::gigaam_transcribe_audio,
            salutespeech::salutespeech_is_configured,
            salutespeech::salutespeech_can_be_selected,
            calendar::local_outlook::local_outlook_calendar_status,
            calendar::local_outlook::request_outlook_calendar_permission,
            calendar::local_outlook::get_upcoming_local_outlook_meetings,
            collections::commands::create_collection,
            collections::commands::rename_collection,
            collections::commands::delete_collection,
            collections::commands::add_meeting_to_collection,
            collections::commands::set_collection_meetings,
            collections::commands::list_collections,
            collections::commands::list_collection_candidates,
            collections::commands::save_search,
            collections::commands::suggest_meeting_series,
            collections::commands::accept_series_suggestion,
            collections::commands::set_series_auto_add,
            collections::commands::convert_collection_to_series,
            collections::commands::run_backfill,
            collections::commands::set_app_setting,
            collections::commands::get_app_settings,
            database::managed_defaults::resolve_managed_defaults_migration,
            gateway_identity::refresh_managed_gateway_token,
            start_recording,
            stop_recording,
            is_recording,
            get_transcription_status,
            meeting_detection::get_auto_meeting_detection_status,
            background_capture::get_background_capture_status,
            meeting_detection::report_auto_listening_start,
            meeting_detection::link_auto_listening_meeting,
            meeting_detection::get_capture_retention_policy,
            meeting_detection::update_capture_retention_policy,
            meeting_detection::list_meeting_windows,
            meeting_detection::review_meeting_window,
            meeting_detection::split_meeting_window,
            meeting_detection::merge_meeting_windows,
            audio::export::get_meeting_audio_path,
            audio::export::get_meeting_audio_playback_info,
            audio::transcription_provenance::get_meeting_transcription_provenance,
            audio::export::export_meeting_audio_mp3,
            save_transcript,
            analytics::commands::init_analytics,
            analytics::commands::get_analytics_device_id,
            analytics::commands::disable_analytics,
            analytics::commands::track_event,
            analytics::commands::identify_user,
            analytics::commands::track_meeting_started,
            analytics::commands::track_recording_started,
            analytics::commands::track_recording_stopped,
            analytics::commands::track_meeting_deleted,
            analytics::commands::track_settings_changed,
            analytics::commands::track_feature_used,
            analytics::commands::is_analytics_enabled,
            analytics::commands::start_analytics_session,
            analytics::commands::end_analytics_session,
            analytics::commands::track_daily_active_user,
            analytics::commands::track_user_first_launch,
            analytics::commands::is_analytics_session_active,
            analytics::commands::track_summary_generation_started,
            analytics::commands::track_summary_generation_completed,
            analytics::commands::track_summary_regenerated,
            analytics::commands::track_model_changed,
            analytics::commands::track_custom_prompt_used,
            analytics::commands::track_meeting_ended,
            analytics::commands::track_analytics_enabled,
            analytics::commands::track_analytics_disabled,
            analytics::commands::track_analytics_transparency_viewed,
            whisper_engine::commands::whisper_init,
            whisper_engine::commands::whisper_get_available_models,
            whisper_engine::commands::whisper_load_model,
            whisper_engine::commands::whisper_get_current_model,
            whisper_engine::commands::whisper_is_model_loaded,
            whisper_engine::commands::whisper_has_available_models,
            whisper_engine::commands::whisper_validate_model_ready,
            whisper_engine::commands::whisper_transcribe_audio,
            whisper_engine::commands::whisper_get_models_directory,
            whisper_engine::commands::whisper_download_model,
            whisper_engine::commands::whisper_cancel_download,
            whisper_engine::commands::whisper_delete_corrupted_model,
            // Parakeet engine commands
            parakeet_engine::commands::parakeet_init,
            parakeet_engine::commands::parakeet_get_available_models,
            parakeet_engine::commands::parakeet_load_model,
            parakeet_engine::commands::parakeet_get_current_model,
            parakeet_engine::commands::parakeet_is_model_loaded,
            parakeet_engine::commands::parakeet_has_available_models,
            parakeet_engine::commands::parakeet_validate_model_ready,
            parakeet_engine::commands::parakeet_transcribe_audio,
            parakeet_engine::commands::parakeet_get_models_directory,
            parakeet_engine::commands::parakeet_download_model,
            parakeet_engine::commands::parakeet_retry_download,
            parakeet_engine::commands::parakeet_cancel_download,
            parakeet_engine::commands::parakeet_delete_corrupted_model,
            parakeet_engine::commands::open_parakeet_models_folder,
            // Parallel processing commands
            whisper_engine::parallel_commands::initialize_parallel_processor,
            whisper_engine::parallel_commands::start_parallel_processing,
            whisper_engine::parallel_commands::pause_parallel_processing,
            whisper_engine::parallel_commands::resume_parallel_processing,
            whisper_engine::parallel_commands::stop_parallel_processing,
            whisper_engine::parallel_commands::get_parallel_processing_status,
            whisper_engine::parallel_commands::get_system_resources,
            whisper_engine::parallel_commands::check_resource_constraints,
            whisper_engine::parallel_commands::calculate_optimal_workers,
            whisper_engine::parallel_commands::prepare_audio_chunks,
            whisper_engine::parallel_commands::test_parallel_processing_setup,
            get_audio_devices,
            trigger_microphone_permission,
            start_recording_with_devices,
            start_recording_with_devices_and_meeting,
            start_audio_level_monitoring,
            stop_audio_level_monitoring,
            is_audio_level_monitoring,
            get_current_microphone_level,
            // Recording pause/resume commands
            audio::recording_commands::pause_recording,
            audio::recording_commands::resume_recording,
            audio::recording_commands::is_recording_paused,
            audio::recording_commands::get_recording_state,
            audio::recording_commands::get_meeting_folder_path,
            // Reload sync commands (retrieve transcript history and meeting name)
            audio::recording_commands::get_transcript_history,
            audio::recording_commands::get_recording_meeting_name,
            // Device monitoring commands (AirPods/Bluetooth disconnect/reconnect)
            audio::recording_commands::poll_audio_device_events,
            audio::recording_commands::get_reconnection_status,
            audio::recording_commands::attempt_device_reconnect,
            // Playback device detection (Bluetooth warning)
            audio::recording_commands::get_active_audio_output,
            // Audio recovery commands (for transcript recovery feature)
            audio::incremental_saver::recover_audio_from_checkpoints,
            audio::incremental_saver::cleanup_checkpoints,
            audio::incremental_saver::has_audio_checkpoints,
            audio::incremental_saver::has_recoverable_audio,
            console_utils::show_console,
            console_utils::hide_console,
            console_utils::toggle_console,
            ollama::get_ollama_models,
            ollama::pull_ollama_model,
            ollama::delete_ollama_model,
            ollama::get_ollama_model_context,
            openai::openai::get_openai_models,
            anthropic::anthropic::get_anthropic_models,
            groq::groq::get_groq_models,
            api::api_get_meetings,
            api::api_get_meeting_memory_config,
            api::api_set_meeting_memory_config,
            api::api_search_transcripts,
            api::api_get_profile,
            api::api_save_profile,
            api::api_update_profile,
            api::api_get_model_config,
            api::api_save_model_config,
            api::api_get_api_key,
            // api::api_get_auto_generate_setting,
            // api::api_save_auto_generate_setting,
            api::api_get_transcript_config,
            api::api_save_transcript_config,
            api::api_get_transcript_api_key,
            api::api_delete_meeting,
            api::api_get_meeting,
            api::api_get_meeting_metadata,
            api::api_get_meeting_transcripts,
            api::api_save_meeting_title,
            api::api_save_transcript,
            api::open_meeting_folder,
            api::test_backend_connection,
            api::debug_backend_connection,
            api::open_external_url,
            // Custom OpenAI commands
            api::api_save_custom_openai_config,
            api::api_get_custom_openai_config,
            api::api_test_custom_openai_connection,
            // Summary commands
            summary::commands::api_process_transcript,
            summary::commands::api_get_summary,
            summary::commands::api_save_meeting_summary,
            summary::commands::api_get_meeting_summary_language,
            summary::commands::api_save_meeting_summary_language,
            summary::commands::api_get_meeting_detected_summary_language,
            summary::commands::api_save_meeting_detected_summary_language,
            summary::commands::api_detect_transcript_summary_language,
            summary::commands::api_cancel_summary,
            // Deep Analytics report commands
            report::commands::generate_analytics_report,
            report::commands::get_analytics_report,
            report::commands::get_meeting_analytics_sections,
            report::commands::open_analytics_report,
            report::commands::cancel_analytics_report,
            report::commands::submit_analytics_answers,
            report::commands::reveal_report_in_folder,
            report::commands::download_analytics_report,
            // Telegram sharing
            telegram::commands::telegram_share_text,
            telegram::commands::save_summary_markdown_file,
            summary::content_window::get_meeting_content_window_suggestion,
            summary::content_window::set_meeting_content_window_preference,
            summary::standup_workflow::list_standup_records,
            summary::standup_workflow::review_standup_record,
            summary::standup_workflow::set_standup_action_status,
            summary::standup_workflow::get_standup_prebrief,
            summary::standup_workflow::get_standup_series_digest,
            summary::corpus_runner::start_standup_corpus_run,
            summary::standup_notes::list_standup_private_notes,
            summary::standup_notes::create_standup_private_note,
            summary::standup_notes::set_standup_private_note_status,
            summary::standup_suggestion::suggest_summary_template,
            summary::interview_workflow::get_interview_config,
            summary::interview_workflow::save_interview_config,
            summary::interview_workflow::get_interview_privacy,
            summary::interview_workflow::save_interview_privacy,
            summary::interview_workflow::list_interview_records,
            summary::interview_workflow::review_interview_record,
            summary::interview_workflow::save_interview_debrief,
            summary::interview_workflow::list_interview_debriefs,
            summary::interview_workflow::create_interview_track,
            summary::interview_workflow::assign_interview_stage,
            summary::interview_workflow::get_interview_handoff,
            summary::interview_workflow::export_interview_memory,
            summary::interview_workflow::purge_expired_interview_memories,
            summary::one_on_one_workflow::get_one_on_one_config,
            summary::one_on_one_workflow::save_one_on_one_config,
            summary::one_on_one_workflow::get_one_on_one_privacy,
            summary::one_on_one_workflow::save_one_on_one_privacy,
            summary::one_on_one_workflow::list_one_on_one_records,
            summary::one_on_one_workflow::review_one_on_one_record,
            summary::one_on_one_workflow::list_one_on_one_commitments,
            summary::one_on_one_workflow::set_one_on_one_commitment_status,
            summary::one_on_one_workflow::set_one_on_one_topic_status,
            summary::one_on_one_workflow::list_one_on_one_private_notes,
            summary::one_on_one_workflow::save_one_on_one_private_note,
            summary::one_on_one_workflow::delete_one_on_one_private_note,
            summary::one_on_one_workflow::share_one_on_one_private_note_to_agenda,
            summary::one_on_one_workflow::list_one_on_one_live_markers,
            summary::one_on_one_workflow::add_one_on_one_live_marker,
            summary::one_on_one_workflow::get_one_on_one_prebrief,
            summary::one_on_one_workflow::list_one_on_one_recurring_suggestions,
            summary::one_on_one_workflow::confirm_one_on_one_recurring_topic,
            summary::one_on_one_workflow::export_one_on_one_accepted_memory,
            summary::one_on_one_workflow::delete_one_on_one_series_memory,
            // Template commands
            summary::template_commands::api_list_templates,
            summary::template_commands::api_get_template_details,
            summary::template_commands::api_validate_template,
            // Built-in AI commands
            summary::summary_engine::commands::builtin_ai_list_models,
            summary::summary_engine::commands::builtin_ai_get_model_info,
            summary::summary_engine::commands::builtin_ai_download_model,
            summary::summary_engine::commands::builtin_ai_cancel_download,
            summary::summary_engine::commands::builtin_ai_delete_model,
            summary::summary_engine::commands::builtin_ai_is_model_ready,
            summary::summary_engine::commands::builtin_ai_get_available_summary_model,
            summary::summary_engine::commands::builtin_ai_get_recommended_model,
            openrouter::get_openrouter_models,
            audio::recording_preferences::get_recording_preferences,
            audio::recording_preferences::set_recording_preferences,
            audio::recording_preferences::get_default_recordings_folder_path,
            audio::recording_preferences::open_recordings_folder,
            audio::recording_preferences::select_recording_folder,
            audio::recording_preferences::get_available_audio_backends,
            audio::recording_preferences::get_current_audio_backend,
            audio::recording_preferences::set_audio_backend,
            audio::recording_preferences::get_audio_backend_info,
            // Language preference commands
            set_language_preference,
            // Notification system commands
            notifications::commands::get_notification_settings,
            notifications::commands::set_notification_settings,
            notifications::commands::request_notification_permission,
            notifications::commands::show_notification,
            notifications::commands::show_test_notification,
            notifications::commands::is_dnd_active,
            notifications::commands::get_system_dnd_status,
            notifications::commands::set_manual_dnd,
            notifications::commands::set_notification_consent,
            notifications::commands::clear_notifications,
            notifications::commands::is_notification_system_ready,
            notifications::commands::initialize_notification_manager_manual,
            notifications::commands::test_notification_with_auto_consent,
            notifications::commands::get_notification_stats,
            // System audio capture commands
            audio::system_audio_commands::start_system_audio_capture_command,
            audio::system_audio_commands::list_system_audio_devices_command,
            audio::system_audio_commands::check_system_audio_permissions_command,
            audio::system_audio_commands::start_system_audio_monitoring,
            audio::system_audio_commands::stop_system_audio_monitoring,
            audio::system_audio_commands::get_system_audio_monitoring_status,
            // Screen Recording permission commands
            audio::permissions::check_screen_recording_permission_command,
            audio::permissions::request_screen_recording_permission_command,
            audio::permissions::trigger_system_audio_permission_command,
            // Database import commands
            database::commands::check_first_launch,
            database::commands::select_legacy_database_path,
            database::commands::detect_legacy_database,
            database::commands::check_default_legacy_database,
            database::commands::check_homebrew_database,
            database::commands::import_and_initialize_database,
            database::commands::initialize_fresh_database,
            // Database and Models path commands
            database::commands::get_database_directory,
            database::commands::open_database_folder,
            database::commands::get_meeting_source_title,
            database::commands::set_meeting_participants,
            whisper_engine::commands::open_models_folder,
            // Onboarding commands
            onboarding::get_onboarding_status,
            onboarding::save_onboarding_status_cmd,
            onboarding::reset_onboarding_status_cmd,
            onboarding::onboarding_should_run,
            onboarding::complete_onboarding,
            window_motion::animate_main_window,
            // System settings commands
            #[cfg(target_os = "macos")]
            utils::open_system_settings,
            // Retranscription commands
            audio::retranscription::start_retranscription_command,
            audio::retranscription::cancel_retranscription_command,
            audio::retranscription::is_retranscription_in_progress_command,
            // Import audio commands
            audio::import::select_and_validate_audio_command,
            audio::import::select_and_validate_audio_folder_command,
            audio::import::validate_audio_file_command,
            audio::import::start_import_audio_command,
            audio::import::start_batch_import_audio_command,
            audio::import::start_batch_import_folder_command,
            audio::import::cancel_import_command,
            audio::import::is_import_in_progress_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            match event {
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    tray::focus_main_window(_app_handle);
                }
                tauri::RunEvent::Exit => {
                    log::info!("Application exiting, cleaning up resources...");
                    _app_handle
                        .state::<meeting_detection::AutoMeetingDetectionState>()
                        .stop();
                    tauri::async_runtime::block_on(async {
                        // Finalize an in-progress background capture BEFORE the database
                        // is closed, so quitting mid-call still saves and registers it.
                        let background_capture =
                            _app_handle.state::<background_capture::BackgroundCaptureState>();
                        if background_capture.is_capturing() {
                            log::info!("Finalizing in-progress background capture before exit");
                            background_capture.stop_and_finalize(_app_handle).await;
                        }

                        // Clean up database connection and checkpoint WAL
                        if let Some(app_state) = _app_handle.try_state::<state::AppState>() {
                            log::info!("Starting database cleanup...");
                            if let Err(e) = app_state.db_manager.cleanup().await {
                                log::error!("Failed to cleanup database: {}", e);
                            } else {
                                log::info!("Database cleanup completed successfully");
                            }
                        } else {
                            log::warn!("AppState not available for database cleanup (likely first launch)");
                        }

                        // Clean up sidecar
                        log::info!("Cleaning up sidecar...");
                        if let Err(e) = summary::summary_engine::force_shutdown_sidecar().await {
                            log::error!("Failed to force shutdown sidecar: {}", e);
                        }
                    });
                    log::info!("Application cleanup complete");
                }
                _ => {}
            }
        });
}
