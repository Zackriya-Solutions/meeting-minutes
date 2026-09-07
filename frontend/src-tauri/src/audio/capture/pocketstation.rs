use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Result};
use pocketstation::connector::{
    Connector, ConnectorError, ConnectorErrorCode, ConnectorErrorStage, ConnectorRetryability,
};
use pocketstation::{AudioFrameDuration, Session, SessionEventKind, SessionEventReceive, Source};

use crate::audio::recording_state::{AudioChunk, AudioError, DeviceType, RecordingState};

const START_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs PocketStation capture while Meetily keeps ownership of processing,
/// transcription, recording, and the user interface.
pub struct PocketStationCapture {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl PocketStationCapture {
    pub fn start(state: Arc<RecordingState>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("meetily-pocketstation-capture".to_string())
            .spawn(move || run_capture(state, worker_stop, ready_tx))?;

        match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                stop,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(anyhow!(error))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                stop.store(true, Ordering::Release);
                let _ = worker.join();
                Err(anyhow!(
                    "PocketStation did not start within {} seconds",
                    START_TIMEOUT.as_secs()
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let outcome = worker.join();
                Err(anyhow!("PocketStation stopped during startup: {outcome:?}"))
            }
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(result) => result,
                Err(_) => Err(anyhow!("PocketStation capture thread panicked")),
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for PocketStationCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run_capture(
    state: Arc<RecordingState>,
    stop: Arc<AtomicBool>,
    ready: mpsc::SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    let session = Session::builder()
        .audio_frame_duration(AudioFrameDuration::Ms10)
        .build();
    let chunk_id = Arc::new(AtomicU64::new(0));
    let destination = meetily_destination(
        &session,
        Arc::clone(&state),
        DeviceType::System,
        Arc::clone(&chunk_id),
    )?;
    session.capture(Source::system_audio())?.send(destination)?;

    let mut running = match session.start() {
        Ok(running) => running,
        Err(error) => {
            let message = format!("PocketStation could not start capture: {error}");
            let _ = ready.send(Err(message.clone()));
            return Err(anyhow!(message));
        }
    };
    let _ = ready.send(Ok(()));

    let mut runtime_error = None;
    while !stop.load(Ordering::Acquire) {
        if let SessionEventReceive::Event(event) = running.try_recv_event() {
            match event.kind() {
                SessionEventKind::Source(_)
                | SessionEventKind::Endpoint(_)
                | SessionEventKind::Rollback(_)
                | SessionEventKind::Finalization(_) => {
                    runtime_error = Some(format!(
                        "PocketStation reported a capture failure: {:?}",
                        event.kind()
                    ));
                    break;
                }
                SessionEventKind::Lifecycle(_) | SessionEventKind::Terminal(_) => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    if let Some(error) = runtime_error {
        let outcome = running.cancel();
        state.report_error(AudioError::StreamFailed);
        if !outcome.is_success() {
            return Err(anyhow!(
                "{error}; PocketStation did not cancel cleanly: {outcome:?}"
            ));
        }
        return Err(anyhow!(error));
    }
    let outcome = running.stop();
    if !outcome.is_success() {
        return Err(anyhow!("PocketStation did not stop cleanly: {outcome:?}"));
    }
    Ok(())
}

fn meetily_destination(
    session: &Session,
    state: Arc<RecordingState>,
    device_type: DeviceType,
    chunk_id: Arc<AtomicU64>,
) -> Result<pocketstation::EndpointHandle> {
    let connector = Connector::from_audio_fn(move |frame| {
        if !state.is_recording() {
            return Ok(());
        }

        let samples = mono_samples(frame.samples(), frame.channels())?;
        state
            .send_audio_chunk(AudioChunk {
                data: samples,
                sample_rate: frame.sample_rate_hz(),
                timestamp: state.get_recording_duration().unwrap_or(0.0),
                chunk_id: chunk_id.fetch_add(1, Ordering::Relaxed),
                device_type: device_type.clone(),
            })
            .map_err(|error| delivery_error(error.to_string()))
    })?;
    Ok(session.destination(connector)?)
}

fn mono_samples(samples: &[f32], channels: u8) -> std::result::Result<Vec<f32>, ConnectorError> {
    let channels = usize::from(channels);
    if channels == 0 {
        return Err(delivery_error("PocketStation returned no audio channels"));
    }

    let frames = samples.chunks_exact(channels);
    if !frames.remainder().is_empty() {
        return Err(delivery_error(
            "PocketStation returned an incomplete audio frame",
        ));
    }

    Ok(frames
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect())
}

fn delivery_error(message: impl Into<String>) -> ConnectorError {
    ConnectorError::new(
        ConnectorErrorCode::new("meetily.audio_pipeline_unavailable")
            .expect("the Meetily Connector error code is valid"),
        ConnectorErrorStage::Delivery,
        ConnectorRetryability::Retryable,
        message,
    )
    .expect("the Meetily Connector error message is valid")
}

#[cfg(test)]
mod tests {
    use super::mono_samples;

    #[test]
    fn stereo_frames_are_downmixed_before_meetily_receives_them() {
        assert_eq!(
            mono_samples(&[1.0, -1.0, 0.5, 0.5], 2).expect("valid stereo audio"),
            vec![0.0, 0.5]
        );
    }
}
