// audio/transcription/streaming_worker.rs
//
// Drives a live streaming transcription session for the duration of a recording.
//
// Unlike the chunk worker (`worker.rs`), which VAD-gates audio and calls a
// one-shot `transcribe()` per segment, this runner holds a persistent provider
// session: it pushes the continuous (pre-VAD) mixed stream up and forwards the
// provider's partial/final events onto the SAME `transcript-update` event the
// chunk worker uses — so history persistence and the UI work unchanged.
//
// Lifecycle / clean shutdown is by channel drop-chain:
//   pipeline stops → `streaming_receiver` closes → audio task drops `audio_tx`
//   → provider flushes the tail + requests the final transcript → provider drops
//   the events sender → the event loop ends → this task's JoinHandle completes.
// `stop_recording` awaits that handle (stored in `TRANSCRIPTION_TASK`), so the
// final transcript is guaranteed to land before the recording is finalized.

use std::sync::Arc;
use std::time::Instant;

use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::streaming_provider::{StreamTranscriptEvent, StreamingTranscriptionProvider};
use super::worker::TranscriptUpdate;
use crate::audio::AudioChunk;

/// Sample rate the streaming providers expect (16 kHz mono).
const STREAMING_SAMPLE_RATE: u32 = 16000;

/// Start a streaming transcription session. Returns a handle that completes once
/// the provider session has fully finalized (final transcript delivered).
///
/// * `streaming_receiver` — continuous mixed audio from the pipeline tap (48 kHz).
/// * `provider` — the configured streaming provider (e.g. Voxtral realtime).
pub fn run_streaming_session<R: Runtime>(
    app: AppHandle<R>,
    streaming_receiver: mpsc::UnboundedReceiver<AudioChunk>,
    provider: Arc<dyn StreamingTranscriptionProvider>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "🌊 Starting streaming transcription session via '{}'",
            provider.provider_name()
        );

        // Open the session. `language: None` — Voxtral auto-detects; other
        // providers may honor a hint later.
        let (events_tx, events_rx) = mpsc::unbounded_channel::<StreamTranscriptEvent>();
        let session = match provider.start_session(None, events_tx).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to start streaming transcription session: {}", e);
                let _ = app.emit(
                    "transcription-error",
                    serde_json::json!({
                        "error": e.to_string(),
                        "userMessage": "Recording failed: could not connect to the realtime transcription endpoint. Please check the endpoint in transcription settings.",
                        "actionable": true,
                    }),
                );
                // Drain the audio tap so the pipeline's unbounded channel can't grow.
                drain_audio(streaming_receiver).await;
                return;
            }
        };

        // Task A: resample the 48 kHz mixed tap to 16 kHz mono and push it up.
        let audio_task = tokio::spawn(feed_audio(streaming_receiver, session.audio_tx));

        // Task B (this task): forward provider events onto `transcript-update`.
        forward_events(&app, events_rx).await;

        // The event loop ends when the provider drops its events sender (session
        // fully closed). Join the audio task for a clean teardown.
        let _ = audio_task.await;
        info!("🌊 Streaming transcription session finished");
    })
}

/// Resample the continuous mixed stream to 16 kHz mono and forward it to the
/// provider until the tap closes (end of recording), then drop `audio_tx` to
/// signal end-of-audio.
async fn feed_audio(
    mut streaming_receiver: mpsc::UnboundedReceiver<AudioChunk>,
    audio_tx: mpsc::UnboundedSender<Vec<f32>>,
) {
    while let Some(chunk) = streaming_receiver.recv().await {
        let samples = if chunk.sample_rate != STREAMING_SAMPLE_RATE {
            crate::audio::audio_processing::resample_audio(
                &chunk.data,
                chunk.sample_rate,
                STREAMING_SAMPLE_RATE,
            )
        } else {
            chunk.data
        };
        if samples.is_empty() {
            continue;
        }
        if audio_tx.send(samples).is_err() {
            // Provider worker gone — nothing more we can do.
            warn!("Streaming provider closed the audio channel; stopping audio feed");
            break;
        }
    }
    // `audio_tx` dropped here → provider flushes the tail and finalizes.
}

/// Consume provider events and emit `transcript-update`s.
///
/// The provider streams a single, ever-growing cumulative text (partials) plus a
/// final. We break it into sentences: the **in-progress** sentence is emitted as
/// a live partial that keeps updating under one `sequence_id` (word-for-word),
/// and when it completes (sentence-final punctuation, soft cap, or end of stream)
/// it is re-emitted as final under that same id and the next sentence takes the
/// next id. The live UI upserts by `sequence_id`, so a segment grows in place and
/// then locks — like a realtime caption that settles into the transcript.
async fn forward_events<R: Runtime>(
    app: &AppHandle<R>,
    mut events_rx: mpsc::UnboundedReceiver<StreamTranscriptEvent>,
) {
    let session_start = Instant::now();
    let mut segmenter = Segmenter::new();
    // Recording-relative start time of the sentence currently being built.
    let mut segment_start: f64 = 0.0;
    let mut speech_announced = false;

    while let Some(ev) = events_rx.recv().await {
        let (text, is_final) = match ev {
            StreamTranscriptEvent::Partial { text } => (text, false),
            StreamTranscriptEvent::Final { text, .. } => (text, true),
            StreamTranscriptEvent::Error { message, fatal } => {
                if fatal {
                    // Transcription is over for this recording. `transcription-error`
                    // is the event the UI actually surfaces to the user (see
                    // useModalState); a `transcription-warning` would be swallowed.
                    error!("Streaming transcription failed: {}", message);
                    let _ = app.emit(
                        "transcription-error",
                        serde_json::json!({
                            "error": message,
                            "userMessage": message,
                            "actionable": true,
                        }),
                    );
                } else {
                    warn!("Streaming transcription warning: {}", message);
                    let _ = app.emit(
                        "transcription-warning",
                        serde_json::json!({ "error": message }),
                    );
                }
                continue;
            }
        };

        for e in segmenter.advance(&text, is_final) {
            announce_speech(app, &mut speech_announced);
            let now = session_start.elapsed().as_secs_f64();
            if !e.is_partial {
                info!("🌊 streaming segment {}: {}", e.seq, e.text);
            }
            emit_update(app, e.text, e.is_partial, 1.0, e.seq, segment_start, now);
            // A finalized sentence closes its block; the next one starts now.
            if !e.is_partial {
                segment_start = now;
            }
        }

        // A final closes the utterance; the next one starts fresh cumulative text.
        if is_final {
            segmenter.reset();
        }
    }
}

/// Soft cap: finalize an in-progress run of speech that hasn't hit sentence-ending
/// punctuation yet, so spontaneous speech without periods still settles into the
/// transcript instead of growing unboundedly.
const SOFT_FLUSH_CHARS: usize = 200;

/// One emission destined for a `transcript-update`: a `sequence_id`, its text, and
/// whether it's still provisional (`is_partial`) or the settled sentence.
#[derive(Debug, PartialEq)]
struct Emission {
    seq: u64,
    text: String,
    is_partial: bool,
}

/// Turns a provider's growing cumulative transcript into per-sentence emissions.
///
/// `committed` is how far into the cumulative text has already been *finalized*;
/// `seq` is the id of the sentence currently being built. On each update, any
/// sentences that completed (at `.?!…`, or a word boundary past
/// [`SOFT_FLUSH_CHARS`]) are emitted as finals (incrementing `seq`), and the
/// remaining tail is emitted as a live partial under the current `seq`.
struct Segmenter {
    committed: usize,
    seq: u64,
}

impl Segmenter {
    fn new() -> Self {
        Self { committed: 0, seq: 0 }
    }

    fn advance(&mut self, text: &str, is_final: bool) -> Vec<Emission> {
        // Cumulative text only grows within an utterance; if it shrank (a new
        // utterance began), restart the byte offset. `seq` keeps climbing so ids
        // stay globally unique.
        if self.committed > text.len() {
            self.committed = 0;
        }
        let mut out = Vec::new();

        // Finalize every sentence that has completed.
        while let Some(b) = next_break(&text[self.committed..]) {
            let seg = text[self.committed..self.committed + b].trim();
            if !seg.is_empty() {
                out.push(Emission { seq: self.seq, text: seg.to_string(), is_partial: false });
                self.seq += 1;
            }
            self.committed += b;
        }

        // The unfinished remainder: a live partial mid-stream, or a final flush.
        let tail = text[self.committed..].trim();
        if !tail.is_empty() {
            out.push(Emission { seq: self.seq, text: tail.to_string(), is_partial: !is_final });
            if is_final {
                self.seq += 1;
            }
        }
        if is_final {
            self.committed = text.len();
        }
        out
    }

    /// End of an utterance: the next one restarts the byte offset (ids keep going).
    fn reset(&mut self) {
        self.committed = 0;
    }
}

/// Byte offset (within `tail`) to cut the next segment: right after the first
/// sentence-final punctuation, or—if the tail is longer than [`SOFT_FLUSH_CHARS`]
/// with none—at the last word boundary within that window. `None` if the tail
/// isn't ready to flush yet. The returned offset is always ≥ 1 and lands on a
/// UTF-8 char boundary (cuts are at ASCII punctuation or spaces).
fn next_break(tail: &str) -> Option<usize> {
    for (i, c) in tail.char_indices() {
        if matches!(c, '.' | '!' | '?' | '…') {
            return Some(i + c.len_utf8());
        }
    }
    if tail.len() > SOFT_FLUSH_CHARS {
        if let Some(sp) = tail[..SOFT_FLUSH_CHARS].rfind(' ') {
            if sp > 0 {
                return Some(sp + 1);
            }
        }
        return Some(tail.len());
    }
    None
}

/// Emit the first `speech-detected` event of the session (UI feedback), once.
fn announce_speech<R: Runtime>(app: &AppHandle<R>, announced: &mut bool) {
    if *announced {
        return;
    }
    *announced = true;
    let _ = app.emit(
        "speech-detected",
        serde_json::json!({ "message": "Speech activity detected" }),
    );
}

/// Build and emit a `transcript-update` matching the chunk worker's shape.
fn emit_update<R: Runtime>(
    app: &AppHandle<R>,
    text: String,
    is_partial: bool,
    confidence: f32,
    sequence_id: u64,
    audio_start_time: f64,
    audio_end_time: f64,
) {
    let update = TranscriptUpdate {
        text,
        timestamp: format_current_timestamp(),
        source: "Audio".to_string(),
        sequence_id,
        chunk_start_time: audio_start_time,
        is_partial,
        confidence,
        audio_start_time,
        audio_end_time,
        duration: (audio_end_time - audio_start_time).max(0.0),
    };
    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit streaming transcript update: {}", e);
    }
}

/// Drain and discard the audio tap (used when the session failed to start), so
/// the pipeline's unbounded sender does not accumulate for the whole recording.
async fn drain_audio(mut streaming_receiver: mpsc::UnboundedReceiver<AudioChunk>) {
    while streaming_receiver.recv().await.is_some() {}
}

/// Wall-clock HH:MM:SS for the display timestamp (matches `worker.rs`).
fn format_current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let hours = (now.as_secs() / 3600) % 24;
    let minutes = (now.as_secs() / 60) % 60;
    let seconds = now.as_secs() % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial(seq: u64, text: &str) -> Emission {
        Emission { seq, text: text.to_string(), is_partial: true }
    }
    fn final_(seq: u64, text: &str) -> Emission {
        Emission { seq, text: text.to_string(), is_partial: false }
    }

    #[test]
    fn in_progress_sentence_updates_in_place_then_finalizes() {
        let mut s = Segmenter::new();
        // The current sentence grows as a live partial under one id (0).
        assert_eq!(s.advance("Hello", false), vec![partial(0, "Hello")]);
        assert_eq!(s.advance("Hello world", false), vec![partial(0, "Hello world")]);
        // It completes → finalized under the SAME id 0.
        assert_eq!(s.advance("Hello world.", false), vec![final_(0, "Hello world.")]);
        // The next sentence takes id 1, again as a growing partial.
        assert_eq!(s.advance("Hello world. And", false), vec![partial(1, "And")]);
        assert_eq!(
            s.advance("Hello world. And more!", false),
            vec![final_(1, "And more!")]
        );
    }

    #[test]
    fn sentence_completing_with_more_after_it_finalizes_and_starts_next_partial() {
        let mut s = Segmenter::new();
        // "One." finalizes as id 0; " Two" starts as partial id 1 in the same update.
        assert_eq!(
            s.advance("One. Two", false),
            vec![final_(0, "One."), partial(1, "Two")]
        );
    }

    #[test]
    fn multiple_sentences_in_one_update_all_finalize() {
        let mut s = Segmenter::new();
        assert_eq!(
            s.advance("One. Two. Three.", false),
            vec![final_(0, "One."), final_(1, "Two."), final_(2, "Three.")]
        );
    }

    #[test]
    fn final_flushes_unterminated_tail_as_final() {
        let mut s = Segmenter::new();
        assert_eq!(s.advance("A complete one.", false), vec![final_(0, "A complete one.")]);
        // No terminal punctuation on the trailing bit; end-of-stream finalizes it.
        assert_eq!(
            s.advance("A complete one. trailing words", true),
            vec![final_(1, "trailing words")]
        );
    }

    #[test]
    fn soft_flush_finalizes_long_punctuationless_runs_at_a_word_boundary() {
        let mut s = Segmenter::new();
        let long = "word ".repeat(60); // 300 chars, no punctuation
        let out = s.advance(&long, false);
        assert!(!out.is_empty());
        assert!(!out[0].is_partial, "long run should finalize, not stay partial");
        assert!(out[0].text.len() <= SOFT_FLUSH_CHARS);
        assert!(out[0].text.starts_with("word"));
    }

    #[test]
    fn next_break_finds_terminal_punctuation() {
        assert_eq!(next_break("Hi there. rest"), Some("Hi there.".len()));
        assert_eq!(next_break("Was?! next"), Some("Was?".len()));
        assert_eq!(next_break("no boundary yet"), None);
    }

    #[test]
    fn utf8_is_not_split_mid_char() {
        // German umlauts are multi-byte; cuts must land on char boundaries.
        let mut s = Segmenter::new();
        let out = s.advance("Schöne Grüße. Nächster", false);
        assert_eq!(out, vec![final_(0, "Schöne Grüße."), partial(1, "Nächster")]);
    }
}
