use crate::api::TranscriptSegment;
use anyhow::Result;
use log::{debug, info};
use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use uuid::Uuid;

static ENGINE_LIFECYCLE_LOCK: Lazy<Arc<AsyncMutex<()>>> =
    Lazy::new(|| Arc::new(AsyncMutex::new(())));

pub(crate) async fn acquire_engine_lifecycle_lock() -> OwnedMutexGuard<()> {
    ENGINE_LIFECYCLE_LOCK.clone().lock_owned().await
}

/// Held by every batch job (retranscription, import, speaker-identification splitting) from
/// engine load to unload, so one job cannot unload or swap the model another is using.
static BATCH_ENGINE_LOCK: Lazy<Arc<AsyncMutex<()>>> =
    Lazy::new(|| Arc::new(AsyncMutex::new(())));

pub(crate) async fn acquire_batch_engine_lock() -> OwnedMutexGuard<()> {
    BATCH_ENGINE_LOCK.clone().lock_owned().await
}

/// True while a batch job holds the transcription engine.
pub(crate) fn batch_engine_busy() -> bool {
    BATCH_ENGINE_LOCK.try_lock().is_err()
}

/// Speaker identification settings for retranscription and import.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpeakerOptions {
    pub identify: bool,
    pub num_speakers: Option<usize>,
}

impl SpeakerOptions {
    pub fn from_command(identify: Option<bool>, num_speakers: Option<u32>) -> Self {
        Self { identify: identify.unwrap_or(false), num_speakers: speaker_count_from_command(num_speakers) }
    }
}

/// The speaker count a command asked for; None (automatic) when it gave none or 0.
pub fn speaker_count_from_command(num_speakers: Option<u32>) -> Option<usize> {
    num_speakers.filter(|n| *n > 0).map(|n| n as usize)
}

/// Unload the transcription engine after a batch job (import or retranscription).
/// Skips unloading if a live recording is currently in progress, since recording
/// uses the same global engine instances.
pub(crate) async fn unload_engine_after_batch(use_parakeet: bool) {
    let _engine_lifecycle_guard = acquire_engine_lifecycle_lock().await;

    if crate::audio::recording_commands::is_recording().await {
        log::info!("Skipping model unload after batch: recording in progress");
        return;
    }

    if use_parakeet {
        use crate::parakeet_engine::commands::PARAKEET_ENGINE;
        let engine = {
            let guard = PARAKEET_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().cloned()
        };
        if let Some(e) = engine {
            e.unload_model().await;
        }
    } else {
        use crate::whisper_engine::commands::WHISPER_ENGINE;
        let engine = {
            let guard = WHISPER_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().cloned()
        };
        if let Some(e) = engine {
            e.unload_model().await;
        }
    }
}

/// A loaded local transcription engine for batch jobs.
pub(crate) enum BatchEngine {
    Whisper(Arc<crate::whisper_engine::WhisperEngine>),
    Parakeet(Arc<crate::parakeet_engine::ParakeetEngine>),
}

impl BatchEngine {
    pub(crate) fn is_parakeet(&self) -> bool {
        matches!(self, BatchEngine::Parakeet(_))
    }

    pub(crate) async fn transcribe(&self, samples: Vec<f32>, language: Option<String>) -> Result<String> {
        match self {
            BatchEngine::Whisper(e) => {
                let (text, _, _) = e
                    .transcribe_audio_with_confidence(samples, language)
                    .await
                    .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))?;
                Ok(text)
            }
            BatchEngine::Parakeet(e) => e
                .transcribe_audio(samples)
                .await
                .map_err(|e| anyhow::anyhow!("Parakeet transcription failed: {}", e)),
        }
    }
}

/// Create transcript segments from transcription results.
/// Each tuple is (text, start_ms, end_ms) from VAD timestamps.
pub(crate) fn create_transcript_segments(transcripts: &[(String, f64, f64)]) -> Vec<TranscriptSegment> {
    transcripts
        .iter()
        .map(|(text, start_ms, end_ms)| {
            let start_seconds = start_ms / 1000.0;
            let end_seconds = end_ms / 1000.0;
            let duration = end_seconds - start_seconds;

            TranscriptSegment {
                id: format!("transcript-{}", Uuid::new_v4()),
                text: text.trim().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                audio_start_time: Some(start_seconds),
                audio_end_time: Some(end_seconds),
                duration: Some(duration),
                speaker: None,
            }
        })
        .collect()
}

/// Write transcripts.json to a meeting folder (atomic write with temp file).
/// `speaker_labels` maps speaker keys to the label shown to the user.
pub(crate) fn write_transcripts_json(
    folder: &Path,
    segments: &[TranscriptSegment],
    speaker_labels: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let transcript_path = folder.join("transcripts.json");
    // Unique per call. Rewrites from the database are serialised, but the live recording saver
    // writes this file without that lock.
    let temp_path = folder.join(format!(".transcripts.json.{}.tmp", Uuid::new_v4()));

    let json = serde_json::json!({
        "version": "1.0",
        "last_updated": chrono::Utc::now().to_rfc3339(),
        "total_segments": segments.len(),
        "speakers": speaker_labels,
        "segments": segments.iter().enumerate().map(|(i, s)| {
            serde_json::json!({
                "id": s.id,
                "text": s.text,
                "timestamp": s.timestamp,
                "audio_start_time": s.audio_start_time,
                "audio_end_time": s.audio_end_time,
                "duration": s.duration,
                "speaker": s.speaker,
                "sequence_id": i
            })
        }).collect::<Vec<_>>()
    });

    let json_string = serde_json::to_string_pretty(&json)?;
    std::fs::write(&temp_path, &json_string)?;
    std::fs::rename(&temp_path, &transcript_path)?;

    info!(
        "Wrote transcripts.json with {} segments to {}",
        segments.len(),
        transcript_path.display()
    );
    Ok(())
}

/// Split a long speech segment at the lowest-energy (silence) point near the target size.
///
/// Scans for 100ms windows with minimal RMS energy within +/-3 seconds of each target
/// split point. If no clear silence is found, falls back to a 1-second overlap split
/// to avoid cutting words at boundaries.
pub(crate) fn split_segment_at_silence(
    segment: &crate::audio::vad::SpeechSegment,
    max_samples: usize,
) -> Vec<crate::audio::vad::SpeechSegment> {
    const SAMPLE_RATE: usize = 16000;
    // 100ms window for energy measurement (1600 samples at 16kHz)
    const ENERGY_WINDOW: usize = SAMPLE_RATE / 10;
    // Search +/-3 seconds around the target split point
    const SEARCH_RADIUS: usize = SAMPLE_RATE * 3;
    // RMS threshold below which we consider a window "silent"
    const SILENCE_RMS_THRESHOLD: f32 = 0.02;
    // Overlap to use when no silence boundary is found (1 second)
    const FALLBACK_OVERLAP: usize = SAMPLE_RATE;

    let total = segment.samples.len();
    if total <= max_samples {
        return vec![segment.clone()];
    }

    let ms_per_sample = (segment.end_timestamp_ms - segment.start_timestamp_ms)
        / segment.samples.len() as f64;
    let mut result = Vec::new();
    let mut pos = 0usize;

    while pos < total {
        let remaining = total - pos;
        if remaining <= max_samples {
            // Last chunk - take everything remaining
            let chunk_samples = segment.samples[pos..].to_vec();
            let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
            let chunk_end_ms = segment.end_timestamp_ms;
            result.push(crate::audio::vad::SpeechSegment {
                samples: chunk_samples,
                start_timestamp_ms: chunk_start_ms,
                end_timestamp_ms: chunk_end_ms,
                confidence: segment.confidence,
            });
            break;
        }

        // Target split point
        let target = pos + max_samples;

        // Search window: [target - SEARCH_RADIUS, target + SEARCH_RADIUS]
        let search_start = target.saturating_sub(SEARCH_RADIUS).max(pos + SAMPLE_RATE);
        let search_end = (target + SEARCH_RADIUS).min(total.saturating_sub(ENERGY_WINDOW));

        // Find the lowest-energy 100ms window in the search range
        let mut best_split = target.min(total); // fallback: exact target
        let mut best_rms = f32::MAX;

        if search_start + ENERGY_WINDOW <= search_end {
            let mut idx = search_start;
            while idx + ENERGY_WINDOW <= search_end {
                let window = &segment.samples[idx..idx + ENERGY_WINDOW];
                let rms = (window.iter().map(|s| s * s).sum::<f32>() / ENERGY_WINDOW as f32).sqrt();
                if rms < best_rms {
                    best_rms = rms;
                    best_split = idx + ENERGY_WINDOW / 2; // split at center of quiet window
                }
                // Step by 10ms (160 samples) for efficiency
                idx += SAMPLE_RATE / 100;
            }
        }

        let split_at = best_split;
        if best_rms <= SILENCE_RMS_THRESHOLD {
            debug!(
                "Splitting at silence boundary: sample {} (RMS={:.4})",
                split_at, best_rms
            );
        } else {
            debug!(
                "No silence found near target (best RMS={:.4}), splitting with overlap at sample {}",
                best_rms, split_at
            );
        }

        // Determine the actual end of this chunk (with overlap if no silence)
        let chunk_end = if best_rms > SILENCE_RMS_THRESHOLD {
            (split_at + FALLBACK_OVERLAP).min(total)
        } else {
            split_at
        };

        let chunk_samples = segment.samples[pos..chunk_end].to_vec();
        let chunk_start_ms = segment.start_timestamp_ms + (pos as f64 * ms_per_sample);
        let chunk_end_ms = segment.start_timestamp_ms + (chunk_end as f64 * ms_per_sample);

        result.push(crate::audio::vad::SpeechSegment {
            samples: chunk_samples,
            start_timestamp_ms: chunk_start_ms,
            end_timestamp_ms: chunk_end_ms,
            confidence: segment.confidence,
        });

        // Advance position to where the current chunk actually ends
        // to avoid transcribing the overlap region twice
        pos = chunk_end;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_options_default_to_off() {
        let o = SpeakerOptions::from_command(None, None);
        assert!(!o.identify);
        let o = SpeakerOptions::from_command(Some(true), Some(3));
        assert!(o.identify);
        assert_eq!(o.num_speakers, Some(3));
        assert_eq!(SpeakerOptions::from_command(Some(true), Some(0)).num_speakers, None);
    }

    #[test]
    fn transcripts_json_includes_speakers() {
        let dir = tempfile::tempdir().unwrap();
        let segments = vec![crate::api::TranscriptSegment {
            id: "t1".into(),
            text: "hello".into(),
            timestamp: "ts".into(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(1.0),
            duration: Some(1.0),
            speaker: Some("spk_0".into()),
        }];
        let mut labels = std::collections::BTreeMap::new();
        labels.insert("spk_0".to_string(), "Noah".to_string());

        write_transcripts_json(dir.path(), &segments, &labels).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("transcripts.json")).unwrap()).unwrap();
        assert_eq!(json["segments"][0]["speaker"], "spk_0");
        assert_eq!(json["speakers"]["spk_0"], "Noah");
    }

    #[tokio::test]
    async fn test_engine_lifecycle_lock_serializes_acquirers() {
        let guard = acquire_engine_lifecycle_lock().await;
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (acquired_tx, mut acquired_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async {
            started_tx.send(()).unwrap();
            let _guard = acquire_engine_lifecycle_lock().await;
            acquired_tx.send(()).unwrap();
        });

        started_rx.await.unwrap();
        assert!(acquired_rx.try_recv().is_err());
        drop(guard);

        acquired_rx.await.unwrap();
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_engine_busy_tracks_the_guard() {
        let guard = acquire_batch_engine_lock().await;
        assert!(batch_engine_busy());
        drop(guard);
        assert!(!batch_engine_busy());
    }
}
