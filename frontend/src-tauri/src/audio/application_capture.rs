use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// The system-audio source used for one recording.
///
/// Global capture is intentionally the default. Application capture is an
/// explicit per-meeting choice and never falls back to the sink monitor.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioCaptureSelection {
    pub mode: AudioCaptureMode,
    pub object_serial: Option<u64>,
    pub application_name: Option<String>,
    pub media_name: Option<String>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioCaptureMode {
    #[default]
    Global,
    Application,
}

impl AudioCaptureSelection {
    pub fn global() -> Self {
        Self::default()
    }

    pub fn is_application(&self) -> bool {
        self.mode == AudioCaptureMode::Application
    }

    pub fn validate(&self) -> Result<()> {
        if !self.is_application() {
            return Ok(());
        }

        if cfg!(not(target_os = "linux")) {
            return Err(anyhow!(
                "Selected application audio capture requires PipeWire and is only available on Linux. Use global system audio on this platform."
            ));
        }

        if self.object_serial.is_none() {
            return Err(anyhow!(
                "No application audio stream was selected. Choose an application/media stream or switch to global system audio."
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplicationAudioStream {
    pub object_serial: u64,
    pub node_id: u32,
    pub application_name: String,
    pub media_name: String,
    pub process_name: String,
    pub node_name: String,
}

/// Pick the capture target from the currently visible streams: the saved
/// object serial wins, but only when the stream's stable identity still
/// matches, because PipeWire serials reset on daemon restart and can
/// collide with an unrelated node. Otherwise the saved identity must match
/// exactly one stream. Ambiguity or absence is an error because selected
/// mode must never capture a different app or fall back silently.
pub fn select_capture_target(
    streams: Vec<ApplicationAudioStream>,
    selection: &AudioCaptureSelection,
) -> Result<ApplicationAudioStream> {
    if let Some(serial) = selection.object_serial {
        if let Some(stream) = streams.iter().find(|stream| stream.object_serial == serial) {
            if has_stable_identity(stream, selection) {
                return Ok(stream.clone());
            }
            log::warn!(
                "PipeWire object serial {} now belongs to {} ({}), not the selected application; attempting identity reacquisition",
                serial,
                stream.application_name,
                stream.process_name
            );
        }
    }

    let matches: Vec<ApplicationAudioStream> = streams
        .into_iter()
        .filter(|stream| {
            selection
                .application_name
                .as_deref()
                .map(|value| value == stream.application_name)
                .unwrap_or(false)
                && selection
                    .media_name
                    .as_deref()
                    .map(|value| value == stream.media_name)
                    .unwrap_or(false)
                && selection
                    .process_name
                    .as_deref()
                    .map(|value| value == stream.process_name)
                    .unwrap_or(false)
        })
        .collect();

    match matches.as_slice() {
        [stream] => {
            log::info!(
                "Reacquired selected PipeWire stream by identity: serial {} -> {}",
                selection.object_serial.unwrap_or_default(),
                stream.object_serial
            );
            Ok(stream.clone())
        }
        [] => Err(anyhow!(
            "The selected application/media stream is unavailable. Switch to global system audio or choose another stream."
        )),
        _ => Err(anyhow!(
            "The selected application exposes multiple matching media streams. Refresh the picker and choose one stream explicitly."
        )),
    }
}

/// Stable identity only: application and process names survive node
/// recreation, while media.name is volatile (tab or track titles) and must
/// not invalidate a serial match.
fn has_stable_identity(stream: &ApplicationAudioStream, selection: &AudioCaptureSelection) -> bool {
    selection
        .application_name
        .as_deref()
        .map(|value| value == stream.application_name)
        .unwrap_or(false)
        && selection
            .process_name
            .as_deref()
            .map(|value| value == stream.process_name)
            .unwrap_or(false)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{ApplicationAudioStream, AudioCaptureSelection};
    use crate::audio::pipeline::AudioCapture;
    use crate::audio::recording_state::{AudioError, DeviceType, RecordingState};
    use anyhow::{anyhow, Context, Result};
    use log::warn;
    use pipewire as pw;
    use pw::node::{Node, NodeInfoRef, NodeListener};
    use pw::properties::properties;
    use pw::registry::GlobalObject;
    use pw::spa::param::audio::{AudioFormat, AudioInfoRaw};
    use pw::spa::pod::Pod;
    use pw::spa::utils::result::AsyncSeq;
    use pw::spa::utils::Direction;
    use std::cell::{Cell, RefCell};
    use std::io::Cursor;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc as std_mpsc, Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const NODE_INTERFACE: &str = "PipeWire:Interface:Node";
    const MEDIA_CLASS: &str = "media.class";
    const APPLICATION_NAME: &str = "application.name";
    const MEDIA_NAME: &str = "media.name";
    const PROCESS_BINARY: &str = "application.process.binary";
    const PROCESS_ID: &str = "application.process.id";
    const NODE_NAME: &str = "node.name";
    const OBJECT_SERIAL: &str = "object.serial";

    pub fn list_application_audio_streams() -> Result<Vec<ApplicationAudioStream>> {
        run_registry_query(|streams| streams.collect::<Vec<_>>())
    }

    /// Resolve the saved serial first. If PipeWire recreated the node, use the
    /// saved identity to reacquire exactly one equivalent stream. Ambiguity is
    /// an error because selected mode must never capture a different app.
    fn resolve_target(selection: &AudioCaptureSelection) -> Result<ApplicationAudioStream> {
        let streams = list_application_audio_streams()?;
        super::select_capture_target(streams, selection)
    }

    pub struct PipeWireAudioStream {
        stop_sender: Option<pw::channel::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl PipeWireAudioStream {
        pub fn start(
            selection: &AudioCaptureSelection,
            state: Arc<RecordingState>,
        ) -> Result<Self> {
            selection.validate()?;
            let target = resolve_target(selection)?;
            let device = Arc::new(crate::audio::devices::AudioDevice::new(
                format!(
                    "{} — {} ({})",
                    target.application_name, target.media_name, target.process_name
                ),
                crate::audio::devices::DeviceType::Output,
            ));
            let capture = AudioCapture::new(device, state, 48000, 2, DeviceType::System, None);

            let (ready_sender, ready_receiver) = std_mpsc::channel::<Result<()>>();
            let (stop_sender, stop_receiver) = pw::channel::channel::<()>();
            let target_serial = target.object_serial;

            let thread = thread::Builder::new()
                .name("meetily-pipewire-capture".to_string())
                .spawn(move || {
                    if let Err(error) = run_capture_thread(
                        target_serial,
                        capture,
                        stop_receiver,
                        ready_sender.clone(),
                    ) {
                        warn!("PipeWire capture thread exited with error: {}", error);
                        let _ = ready_sender.send(Err(error));
                    }
                })
                .context("Failed to start PipeWire capture thread")?;

            match ready_receiver.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => Ok(Self {
                    stop_sender: Some(stop_sender),
                    thread: Some(thread),
                }),
                Ok(Err(error)) => {
                    let _ = stop_sender.send(());
                    let _ = thread.join();
                    Err(error)
                }
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    let _ = stop_sender.send(());
                    let _ = thread.join();
                    Err(anyhow!(
                        "Timed out while connecting to the selected application audio stream"
                    ))
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = thread.join();
                    Err(anyhow!(
                        "The PipeWire capture thread stopped before the selected application audio stream became ready"
                    ))
                }
            }
        }

        pub fn stop(mut self) -> Result<()> {
            if let Some(sender) = self.stop_sender.take() {
                let _ = sender.send(());
            }
            if let Some(thread) = self.thread.take() {
                thread
                    .join()
                    .map_err(|_| anyhow!("PipeWire capture thread panicked"))?;
            }
            Ok(())
        }
    }

    fn run_capture_thread(
        target_serial: u64,
        capture: AudioCapture,
        stop_receiver: pw::channel::Receiver<()>,
        ready_sender: std_mpsc::Sender<Result<()>>,
    ) -> Result<()> {
        pw::init();
        let main_loop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&main_loop, None)?;
        let core = context.connect_rc(None)?;

        let mut props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::NODE_DONT_RECONNECT => "true",
            "node.dont-fallback" => "true",
        };
        props.insert(*pw::keys::TARGET_OBJECT, target_serial.to_string());

        let stream = pw::stream::StreamBox::new(&core, "meetily-application-audio", props)?;
        let mut format = AudioInfoRaw::new();
        format.set_format(AudioFormat::F32LE);
        format.set_rate(48000);
        format.set_channels(2);
        let format_for_connection = format.clone();
        let pod_bytes = serialize_audio_format(format_for_connection)?;
        let pod = Pod::from_bytes(&pod_bytes)
            .ok_or_else(|| anyhow!("Failed to build PipeWire audio format pod"))?;
        let mut params = [pod];

        let reported_error = Arc::new(AtomicBool::new(false));
        let error_state = reported_error.clone();
        let main_loop_for_error = main_loop.clone();
        let user_data = CaptureUserData { capture, format };
        let listener = stream
            .add_local_listener_with_user_data(user_data)
            .state_changed(move |_, data, _, new_state| {
                if let pw::stream::StreamState::Error(message) = new_state {
                    if !error_state.swap(true, Ordering::SeqCst) {
                        data.capture.state().report_error(AudioError::SelectedAudioUnavailable(
                            format!(
                                "Selected application audio stream disappeared: {}. Switch to global system audio.",
                                message
                            ),
                        ));
                    }
                    main_loop_for_error.quit();
                }
            })
            .param_changed(|_, data, id, param| {
                if id == pw::spa::param::ParamType::Format.as_raw() {
                    if let Some(param) = param {
                        if data.format.parse(param).is_err() {
                            warn!("PipeWire selected audio format could not be parsed");
                        }
                    }
                }
            })
            .process(|stream, data| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                for raw_data in buffer.datas_mut() {
                    let chunk = raw_data.chunk();
                    let offset = chunk.offset() as usize;
                    let end = offset.saturating_add(chunk.size() as usize);
                    let Some(bytes) = raw_data.data() else {
                        continue;
                    };
                    if end > bytes.len() || offset >= end {
                        continue;
                    }
                    let samples: Vec<f32> = bytes[offset..end]
                        .chunks_exact(std::mem::size_of::<f32>())
                        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                        .collect();
                    if !samples.is_empty() {
                        data.capture.process_audio_data(&samples);
                    }
                }
            })
            .register()?;

        stream.connect(
            Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )?;

        ready_sender
            .send(Ok(()))
            .map_err(|_| anyhow!("PipeWire capture startup receiver closed"))?;

        let _stop_receiver = stop_receiver.attach(main_loop.loop_(), {
            let main_loop = main_loop.clone();
            move |_| main_loop.quit()
        });

        main_loop.run();
        drop(listener);
        Ok(())
    }

    struct CaptureUserData {
        capture: AudioCapture,
        format: AudioInfoRaw,
    }

    fn serialize_audio_format(format: AudioInfoRaw) -> Result<Vec<u8>> {
        let object = pw::spa::pod::Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: pw::spa::param::ParamType::EnumFormat.as_raw(),
            properties: format.into(),
        };
        Ok(pw::spa::pod::serialize::PodSerializer::serialize(
            Cursor::new(Vec::new()),
            &pw::spa::pod::Value::Object(object),
        )?
        .0
        .into_inner())
    }

    fn run_registry_query<T, F>(build_result: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(std::vec::IntoIter<ApplicationAudioStream>) -> T + Send + 'static,
    {
        let (result_sender, result_receiver) = std_mpsc::channel::<Result<T>>();
        thread::Builder::new()
            .name("meetily-pipewire-enumeration".to_string())
            .spawn(move || {
                let result = (|| -> Result<T> {
                    pw::init();
                    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
                    let context = pw::context::ContextRc::new(&main_loop, None)?;
                    let core = context.connect_rc(None)?;
                    let registry = core.get_registry_rc()?;
                    let streams = Arc::new(Mutex::new(Vec::new()));
                    // Registry globals only carry a partial prop set; the full
                    // identity (media.name, application.process.*) is on the
                    // bound node's info, so bind every matching node and keep
                    // the proxies alive until their info events arrive.
                    let node_bindings: Rc<RefCell<Vec<(Node, NodeListener)>>> =
                        Rc::new(RefCell::new(Vec::new()));
                    let pending_sync: Rc<Cell<Option<AsyncSeq>>> = Rc::new(Cell::new(None));
                    let listener = registry
                        .add_listener_local()
                        .global({
                            let registry = registry.clone();
                            let core = core.clone();
                            let streams = streams.clone();
                            let node_bindings = node_bindings.clone();
                            let pending_sync = pending_sync.clone();
                            move |global| {
                                if !is_application_audio_global(global) {
                                    return;
                                }
                                let node: Node = match registry.bind(global) {
                                    Ok(node) => node,
                                    Err(error) => {
                                        warn!(
                                            "Failed to bind PipeWire node {}: {}",
                                            global.id, error
                                        );
                                        return;
                                    }
                                };
                                let info_listener = node
                                    .add_listener_local()
                                    .info({
                                        let streams = streams.clone();
                                        move |info| {
                                            if let Some(stream) =
                                                application_stream_from_node_info(info)
                                            {
                                                let mut streams = streams.lock().unwrap();
                                                if !streams.iter().any(
                                                    |existing: &ApplicationAudioStream| {
                                                        existing.node_id == stream.node_id
                                                    },
                                                ) {
                                                    streams.push(stream);
                                                }
                                            }
                                        }
                                    })
                                    .register();
                                node_bindings.borrow_mut().push((node, info_listener));
                                // A new round trip after each bind guarantees the
                                // final done event arrives after every info event.
                                match core.sync(0) {
                                    Ok(seq) => pending_sync.set(Some(seq)),
                                    Err(error) => {
                                        warn!("PipeWire core sync failed: {}", error)
                                    }
                                }
                            }
                        })
                        .register();
                    let core_listener = core
                        .add_listener_local()
                        .done({
                            let main_loop = main_loop.clone();
                            let pending_sync = pending_sync.clone();
                            move |id, seq| {
                                if id == pw::core::PW_ID_CORE && pending_sync.get() == Some(seq) {
                                    main_loop.quit();
                                }
                            }
                        })
                        .register();
                    pending_sync.set(Some(core.sync(0)?));
                    // Hard stop in case the server never answers the round trips.
                    let enumeration_timed_out = Rc::new(Cell::new(false));
                    let timer = main_loop.loop_().add_timer({
                        let main_loop = main_loop.clone();
                        let enumeration_timed_out = enumeration_timed_out.clone();
                        move |_| {
                            enumeration_timed_out.set(true);
                            main_loop.quit();
                        }
                    });
                    timer.update_timer(Some(Duration::from_secs(2)), None);
                    main_loop.run();
                    drop(listener);
                    drop(core_listener);
                    node_bindings.borrow_mut().clear();
                    if enumeration_timed_out.get() {
                        return Err(anyhow!(
                            "PipeWire did not answer the application stream enumeration within 2 seconds. Try again or use global system audio."
                        ));
                    }
                    let streams = Arc::try_unwrap(streams)
                        .map_err(|_| anyhow!("PipeWire enumeration state still in use"))?
                        .into_inner()
                        .map_err(|_| anyhow!("PipeWire enumeration state was poisoned"))?;
                    Ok(build_result(streams.into_iter()))
                })();
                let _ = result_sender.send(result);
            })?;

        result_receiver
            .recv_timeout(Duration::from_secs(3))
            .map_err(|error| {
                anyhow!(
                    "Timed out while enumerating PipeWire application streams: {}",
                    error
                )
            })?
    }

    fn is_application_audio_global(global: &GlobalObject<&pw::spa::utils::dict::DictRef>) -> bool {
        global.type_.to_str() == NODE_INTERFACE
            && global
                .props
                .as_ref()
                .and_then(|props| props.get(MEDIA_CLASS))
                == Some("Stream/Output/Audio")
    }

    /// Identity comes from the bound node's info props, never from the
    /// registry global: only the node info carries media.name and
    /// application.process.*, and degraded placeholder identity must not
    /// reach the picker or the reacquisition tuple.
    fn application_stream_from_node_info(info: &NodeInfoRef) -> Option<ApplicationAudioStream> {
        let props = info.props()?;
        if props.get(MEDIA_CLASS)? != "Stream/Output/Audio" {
            return None;
        }
        let serial = props.get(OBJECT_SERIAL)?.parse::<u64>().ok()?;
        let node_name = props.get(NODE_NAME)?.to_string();
        let application_name = props
            .get(APPLICATION_NAME)
            .unwrap_or(node_name.as_str())
            .to_string();
        let media_name = props
            .get(MEDIA_NAME)
            .unwrap_or(node_name.as_str())
            .to_string();
        let process_name = props
            .get(PROCESS_BINARY)
            .map(str::to_string)
            .or_else(|| props.get(PROCESS_ID).map(|pid| format!("process {}", pid)))
            .unwrap_or_else(|| node_name.clone());

        Some(ApplicationAudioStream {
            object_serial: serial,
            node_id: info.id(),
            application_name,
            media_name,
            process_name,
            node_name,
        })
    }

    pub use list_application_audio_streams as list;
    pub use PipeWireAudioStream as Stream;
}

#[cfg(target_os = "linux")]
pub use linux::{list, Stream};

#[cfg(not(target_os = "linux"))]
pub fn list() -> Result<Vec<ApplicationAudioStream>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "linux"))]
pub struct Stream;

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(serial: u64, app: &str, media: &str, process: &str) -> ApplicationAudioStream {
        ApplicationAudioStream {
            object_serial: serial,
            node_id: serial as u32,
            application_name: app.to_string(),
            media_name: media.to_string(),
            process_name: process.to_string(),
            node_name: format!("node-{}", serial),
        }
    }

    fn application_selection(serial: Option<u64>) -> AudioCaptureSelection {
        AudioCaptureSelection {
            mode: AudioCaptureMode::Application,
            object_serial: serial,
            application_name: Some("Chromium".to_string()),
            media_name: Some("Playback".to_string()),
            process_name: Some("vesktop.bin".to_string()),
        }
    }

    #[test]
    fn default_selection_is_global_and_valid() {
        let selection = AudioCaptureSelection::global();
        assert_eq!(selection.mode, AudioCaptureMode::Global);
        assert!(!selection.is_application());
        assert!(selection.validate().is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn application_selection_without_serial_is_rejected() {
        let mut selection = application_selection(None);
        selection.application_name = None;
        selection.media_name = None;
        selection.process_name = None;
        let error = selection.validate().unwrap_err().to_string();
        assert!(error.contains("switch to global system audio"), "{}", error);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn application_selection_with_serial_is_valid() {
        assert!(application_selection(Some(187)).validate().is_ok());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn application_selection_is_rejected_on_unsupported_platforms() {
        let error = application_selection(Some(187))
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains("only available on Linux"), "{}", error);
    }

    #[test]
    fn frontend_global_payload_deserializes() {
        let selection: AudioCaptureSelection =
            serde_json::from_str(r#"{"mode":"global"}"#).unwrap();
        assert_eq!(selection.mode, AudioCaptureMode::Global);
        assert!(selection.object_serial.is_none());
    }

    #[test]
    fn frontend_application_payload_deserializes() {
        let payload = r#"{
            "mode": "application",
            "object_serial": 187,
            "application_name": "Chromium",
            "media_name": "Playback",
            "process_name": "vesktop.bin"
        }"#;
        let selection: AudioCaptureSelection = serde_json::from_str(payload).unwrap();
        assert!(selection.is_application());
        assert_eq!(selection.object_serial, Some(187));
        assert_eq!(selection.application_name.as_deref(), Some("Chromium"));
        assert_eq!(selection.media_name.as_deref(), Some("Playback"));
        assert_eq!(selection.process_name.as_deref(), Some("vesktop.bin"));
    }

    #[test]
    fn select_capture_target_prefers_exact_serial() {
        let streams = vec![
            stream(92, "speech-dispatcher-dummy", "playback", "sd_dummy"),
            stream(187, "Chromium", "Playback", "vesktop.bin"),
        ];
        let target = select_capture_target(streams, &application_selection(Some(187))).unwrap();
        assert_eq!(target.object_serial, 187);
        assert_eq!(target.application_name, "Chromium");
    }

    #[test]
    fn select_capture_target_never_captures_serial_collision_with_different_app() {
        let streams = vec![stream(
            187,
            "speech-dispatcher-dummy",
            "playback",
            "sd_dummy",
        )];
        let error = select_capture_target(streams, &application_selection(Some(187)))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Switch to global system audio"), "{}", error);
    }

    #[test]
    fn select_capture_target_reacquires_by_identity_when_serial_collides() {
        let streams = vec![
            stream(187, "speech-dispatcher-dummy", "playback", "sd_dummy"),
            stream(311, "Chromium", "Playback", "vesktop.bin"),
        ];
        let target = select_capture_target(streams, &application_selection(Some(187))).unwrap();
        assert_eq!(target.object_serial, 311);
        assert_eq!(target.application_name, "Chromium");
    }

    #[test]
    fn select_capture_target_keeps_serial_match_when_only_media_name_changed() {
        let streams = vec![stream(187, "Chromium", "Some Tab Title", "vesktop.bin")];
        let target = select_capture_target(streams, &application_selection(Some(187))).unwrap();
        assert_eq!(target.object_serial, 187);
        assert_eq!(target.media_name, "Some Tab Title");
    }

    #[test]
    fn select_capture_target_reacquires_recreated_node_by_identity() {
        let streams = vec![
            stream(92, "speech-dispatcher-dummy", "playback", "sd_dummy"),
            stream(311, "Chromium", "Playback", "vesktop.bin"),
        ];
        let target = select_capture_target(streams, &application_selection(Some(187))).unwrap();
        assert_eq!(target.object_serial, 311);
        assert_eq!(target.process_name, "vesktop.bin");
    }

    #[test]
    fn select_capture_target_fails_visibly_when_stream_is_gone() {
        let streams = vec![stream(
            92,
            "speech-dispatcher-dummy",
            "playback",
            "sd_dummy",
        )];
        let error = select_capture_target(streams, &application_selection(Some(187)))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Switch to global system audio"), "{}", error);
    }

    #[test]
    fn select_capture_target_rejects_ambiguous_identity_matches() {
        let streams = vec![
            stream(311, "Chromium", "Playback", "vesktop.bin"),
            stream(312, "Chromium", "Playback", "vesktop.bin"),
        ];
        let error = select_capture_target(streams, &application_selection(Some(187)))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("multiple matching media streams"),
            "{}",
            error
        );
    }

    #[test]
    fn select_capture_target_never_matches_without_full_identity() {
        let streams = vec![stream(311, "Chromium", "Playback", "chrome")];
        let selection = AudioCaptureSelection {
            mode: AudioCaptureMode::Application,
            object_serial: Some(187),
            application_name: Some("Chromium".to_string()),
            media_name: None,
            process_name: None,
        };
        assert!(select_capture_target(streams, &selection).is_err());
    }

    /// Executes the real `#[tauri::command]` wrapper through the mock runtime
    /// to pin the IPC argument-key contract: a `capture_selection` parameter is
    /// looked up under the camelCase wire key `captureSelection`, and a
    /// snake_case key silently deserializes to `None`. The frontend invoke
    /// payload in recordingService.ts must therefore use `captureSelection`.
    mod ipc_boundary {
        use super::super::{AudioCaptureMode, AudioCaptureSelection};
        use tauri::ipc::{CallbackFn, InvokeBody};
        use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
        use tauri::webview::InvokeRequest;

        #[tauri::command]
        fn echo_capture_selection(
            capture_selection: Option<AudioCaptureSelection>,
        ) -> AudioCaptureSelection {
            capture_selection.unwrap_or_else(AudioCaptureSelection::global)
        }

        fn invoke_with_body(body: serde_json::Value) -> AudioCaptureSelection {
            let app = mock_builder()
                .invoke_handler(tauri::generate_handler![echo_capture_selection])
                .build(mock_context(noop_assets()))
                .expect("failed to build mock Tauri app");
            let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                .build()
                .expect("failed to build mock webview");
            #[cfg(windows)]
            let local_origin = "http://tauri.localhost";
            #[cfg(not(windows))]
            let local_origin = "tauri://localhost";
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: "echo_capture_selection".to_string(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: local_origin.parse().expect("invalid mock url"),
                    body: InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .expect("echo_capture_selection invoke failed")
            .deserialize::<AudioCaptureSelection>()
            .expect("echo response did not deserialize")
        }

        #[test]
        fn application_selection_crosses_ipc_boundary_with_camel_case_key() {
            let selection = invoke_with_body(serde_json::json!({
                "captureSelection": {
                    "mode": "application",
                    "object_serial": 187,
                    "application_name": "Chromium",
                    "media_name": "Playback",
                    "process_name": "vesktop.bin"
                }
            }));
            assert_eq!(selection.mode, AudioCaptureMode::Application);
            assert_eq!(selection.object_serial, Some(187));
            assert_eq!(selection.application_name.as_deref(), Some("Chromium"));
            assert_eq!(selection.media_name.as_deref(), Some("Playback"));
            assert_eq!(selection.process_name.as_deref(), Some("vesktop.bin"));
        }

        #[test]
        fn snake_case_wire_key_never_reaches_the_command_and_defaults_to_global() {
            let selection = invoke_with_body(serde_json::json!({
                "capture_selection": {
                    "mode": "application",
                    "object_serial": 187
                }
            }));
            assert_eq!(selection.mode, AudioCaptureMode::Global);
            assert!(selection.object_serial.is_none());
        }
    }

    /// Read-only introspection of the live PipeWire graph. Ignored by default
    /// so CI machines without a PipeWire session skip it; run locally with
    /// `cargo test -- --ignored live_pipewire`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn live_pipewire_enumeration_reports_honest_stream_identity() {
        let streams = list().expect("PipeWire registry enumeration failed");
        println!("Discovered {} Stream/Output/Audio node(s):", streams.len());
        for stream in &streams {
            println!(
                "  serial={} app={:?} media={:?} process={:?} node={:?}",
                stream.object_serial,
                stream.application_name,
                stream.media_name,
                stream.process_name,
                stream.node_name
            );
            assert!(!stream.application_name.is_empty());
            assert!(!stream.media_name.is_empty());
            assert!(!stream.process_name.is_empty());
            assert!(!stream.node_name.is_empty());
            assert_ne!(
                stream.media_name, "Audio playback",
                "degraded placeholder media identity must not be shipped"
            );
            assert_ne!(
                stream.process_name, "Unknown process",
                "degraded placeholder process identity must not be shipped"
            );
        }
    }
}
