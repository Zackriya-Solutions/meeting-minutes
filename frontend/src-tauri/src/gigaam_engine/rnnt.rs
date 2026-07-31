//! GigaAM v3 e2e RNN-T: encoder + prediction network (decoder) + joiner, greedy
//! transducer decode. Ported from onnx-asr `GigaamV2Rnnt` / `_AsrWithTransducerDecoding`.
//!
//! The greedy loop mirrors `parakeet_engine`'s: at each frame it runs the prediction
//! network with the last emitted token, then the joiner, and advances the frame on a
//! blank or after `MAX_TOKENS_PER_STEP` emissions. Re-running the decoder every frame
//! (rather than caching it across blanks, as onnx-asr does) yields identical output for
//! a deterministic model — same (token, state) inputs → same result.
//!
//! Interfaces (verified against the `istupakov/gigaam-v3-onnx` int8 exports):
//!   encoder: IN `audio_signal` f32[1,64,T], `length` i64[1]
//!            OUT `encoded` f32[1,768,T'], `encoded_len` i32[1]
//!   decoder: IN `x` i64[1,1], `h.1` f32[1,1,320], `c.1` f32[1,1,320]
//!            OUT `dec` f32[1,1,320], `h` f32[1,1,320], `c` f32[1,1,320]
//!   joiner : IN `enc` f32[1,768,1], `dec` f32[1,320,1]
//!            OUT `joint` f32[1,1,1,V]   (V classes, blank as the last class)

use std::path::Path;

use anyhow::{anyhow, Result};
use ndarray::{Array1, Array2, Array3, ArrayD, IxDyn};

use super::featurizer::{Featurizer, N_MELS};
use super::model::{build_session, find_blank_idx, ids_to_text, load_vocab};

/// LSTM prediction-network hidden size (`h`/`c` last dim).
const PRED_HIDDEN: usize = 320;
/// Cap on tokens emitted at a single encoder frame before forcing a frame advance
/// (GigaAM's `max_tokens_per_step` default).
const MAX_TOKENS_PER_STEP: usize = 3;

/// One decoded word with timing derived from the encoder frame each BPE piece was
/// emitted at, relative to the start of the transcribed waveform. Transducer emissions
/// lag the audio slightly (the joiner fires once enough acoustic evidence accumulated),
/// so treat boundaries as ~±1 frame (40 ms) soft.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Group BPE pieces + their emission frames into [`TimedWord`]s. A piece starting with
/// the sentencepiece word marker `▁` (U+2581) opens a new word; everything else
/// (continuation pieces, punctuation) appends to the current word. `frame_ms` is the
/// duration of one encoder frame.
pub(super) fn tokens_to_timed_words(
    vocab: &[String],
    tokens: &[usize],
    frames: &[usize],
    frame_ms: f64,
) -> Vec<TimedWord> {
    let mut words: Vec<TimedWord> = Vec::new();
    for (&id, &frame) in tokens.iter().zip(frames) {
        let Some(piece) = vocab.get(id) else { continue };
        let start_ms = (frame as f64 * frame_ms).round() as i64;
        let end_ms = ((frame + 1) as f64 * frame_ms).round() as i64;
        let is_word_start = piece.starts_with('\u{2581}');
        let text = piece.replace('\u{2581}', "");
        match words.last_mut() {
            Some(last) if !is_word_start => {
                last.text.push_str(&text);
                last.end_ms = last.end_ms.max(end_ms);
            }
            _ => {
                if text.is_empty() {
                    continue; // a bare "▁" piece carries no word content
                }
                words.push(TimedWord {
                    text,
                    start_ms,
                    end_ms,
                });
            }
        }
    }
    words.retain(|w| !w.text.is_empty());
    words
}

/// Where the encoder runs. The prediction network and joiner are always ONNX (7 MB, and
/// their per-frame cost is negligible); only the encoder is worth moving off the CPU.
enum EncoderBackend {
    /// `v3_e2e_rnnt_encoder(.int8).onnx` through `ort`.
    Onnx(ort::session::Session),
    /// The same weights as an fp16 CoreML MLProgram on the Apple Neural Engine.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Ane(super::coreml::AneEncoder),
}

pub struct RnntModel {
    encoder: EncoderBackend,
    decoder: ort::session::Session,
    joiner: ort::session::Session,
    featurizer: Featurizer,
    vocab: Vec<String>,
    blank: usize,
}

impl RnntModel {
    /// `model_files` order (from `GigaamVariant::model_files`): encoder, decoder, joiner.
    pub fn load(
        encoder_path: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        vocab_path: &Path,
    ) -> Result<Self> {
        let vocab = load_vocab(vocab_path)?;
        let blank = find_blank_idx(&vocab);
        Ok(Self {
            encoder: EncoderBackend::Onnx(build_session(encoder_path)?),
            decoder: build_session(decoder_path)?,
            joiner: build_session(joiner_path)?,
            featurizer: Featurizer::new(),
            vocab,
            blank,
        })
    }

    /// Same model, but with the encoder running as a CoreML fp16 MLProgram on the Apple
    /// Neural Engine (`ane_model_dir` is a compiled `encoder-ane.mlmodelc`). The ONNX
    /// encoder is not needed — and not downloaded — in this mode.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn load_with_ane_encoder(
        ane_model_dir: &Path,
        decoder_path: &Path,
        joiner_path: &Path,
        vocab_path: &Path,
    ) -> Result<Self> {
        let vocab = load_vocab(vocab_path)?;
        let blank = find_blank_idx(&vocab);
        Ok(Self {
            encoder: EncoderBackend::Ane(super::coreml::AneEncoder::load(ane_model_dir)?),
            decoder: build_session(decoder_path)?,
            joiner: build_session(joiner_path)?,
            featurizer: Featurizer::new(),
            vocab,
            blank,
        })
    }

    /// Transcribe a 16 kHz mono waveform to punctuated Russian text.
    pub fn transcribe(&mut self, waveform: &[f32]) -> Result<String> {
        let (tokens, _, _) = self.decode_greedy(waveform)?;
        Ok(ids_to_text(&self.vocab, &tokens))
    }

    /// Transcribe a 16 kHz mono waveform into words with per-word timing (see
    /// [`TimedWord`]). Same greedy decode as [`Self::transcribe`] — identical text,
    /// plus the encoder frame each piece was emitted at.
    pub fn transcribe_with_words(&mut self, waveform: &[f32]) -> Result<Vec<TimedWord>> {
        let (tokens, frames, frame_ms) = self.decode_greedy(waveform)?;
        Ok(tokens_to_timed_words(&self.vocab, &tokens, &frames, frame_ms))
    }

    /// Greedy transducer decode → (token ids, emission encoder-frame per token, ms per
    /// encoder frame).
    fn decode_greedy(&mut self, waveform: &[f32]) -> Result<(Vec<usize>, Vec<usize>, f64)> {
        let (feats, t) = self.featurizer.compute(waveform);
        if t == 0 {
            return Ok((Vec::new(), Vec::new(), 0.0));
        }

        // Encoder → owned `encoded` [1,D,T'] (row-major), dims, and valid length.
        let (encoded, enc_dim, enc_tp, enc_len) = self.encode(feats, t)?;
        // Feature frames are HOP samples (10 ms) apart; the encoder subsamples T → T',
        // so one encoder frame spans t/T' feature frames (4× ⇒ 40 ms for GigaAM v3).
        let feature_frame_ms =
            super::featurizer::HOP as f64 / super::featurizer::SAMPLE_RATE as f64 * 1000.0;
        let frame_ms = if enc_tp > 0 {
            t as f64 * feature_frame_ms / enc_tp as f64
        } else {
            feature_frame_ms
        };

        // Greedy transducer decode.
        let mut h: ArrayD<f32> = ArrayD::zeros(IxDyn(&[1, 1, PRED_HIDDEN]));
        let mut c: ArrayD<f32> = ArrayD::zeros(IxDyn(&[1, 1, PRED_HIDDEN]));
        let mut tokens: Vec<usize> = Vec::new();
        let mut frames: Vec<usize> = Vec::new();
        let mut t_idx = 0usize;
        let mut emitted = 0usize;

        while t_idx < enc_len {
            let last_token = tokens.last().copied().unwrap_or(self.blank) as i64;
            // encoded is [1, D, T'] row-major: element [0,d,t] = d*T' + t.
            let enc_frame: Vec<f32> = (0..enc_dim).map(|d| encoded[d * enc_tp + t_idx]).collect();

            let (logits, h_new, c_new) =
                self.decode_step(last_token, &h, &c, enc_frame, enc_dim)?;
            let token = argmax(&logits);

            if token != self.blank {
                // Commit the prediction-network state only when a real token is emitted.
                h = h_new;
                c = c_new;
                tokens.push(token);
                frames.push(t_idx);
                emitted += 1;
            }
            if token == self.blank || emitted >= MAX_TOKENS_PER_STEP {
                t_idx += 1;
                emitted = 0;
            }
        }
        Ok((tokens, frames, frame_ms))
    }

    /// Run the encoder on log-mel `features` (row-major `[N_MELS][frames]`, taken by value so
    /// the ONNX path can wrap it without a copy). Returns owned `encoded` [1,D,T'] flattened
    /// row-major, D, T', and the valid time length (clamped to T'), whichever backend is
    /// loaded.
    fn encode(&mut self, features: Vec<f32>, frames: usize) -> Result<(Vec<f32>, usize, usize, usize)> {
        match &mut self.encoder {
            EncoderBackend::Onnx(_) => {
                let features = Array3::from_shape_vec((1, N_MELS, frames), features)
                    .map_err(|e| anyhow!("feature reshape: {e}"))?;
                let length = Array1::from_vec(vec![frames as i64]);
                self.encode_onnx(&features, &length)
            }
            // The CoreML graph has a fixed window, so `encode_sequence` chunks longer
            // sequences and reports the concatenated valid length — there is no separate
            // `encoded_len` output to trust (it is off by one on zero-padded input).
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            EncoderBackend::Ane(encoder) => {
                let (encoded, dim, tp) = encoder.encode_sequence(&features, frames)?;
                Ok((encoded, dim, tp, tp))
            }
        }
    }

    /// The ONNX encoder path.
    fn encode_onnx(
        &mut self,
        features: &Array3<f32>,
        length: &Array1<i64>,
    ) -> Result<(Vec<f32>, usize, usize, usize)> {
        use ort::inputs;
        use ort::value::TensorRef;

        let EncoderBackend::Onnx(session) = &mut self.encoder else {
            return Err(anyhow!("encode_onnx called without an ONNX encoder"));
        };
        let f_ref = TensorRef::from_array_view(features.view())
            .map_err(|e| anyhow!("enc features: {e}"))?;
        let l_ref =
            TensorRef::from_array_view(length.view()).map_err(|e| anyhow!("enc length: {e}"))?;
        let out = session
            .run(inputs!["audio_signal" => f_ref, "length" => l_ref])
            .map_err(|e| anyhow!("encoder run: {e}"))?;

        let enc = out
            .get("encoded")
            .ok_or_else(|| anyhow!("encoder output 'encoded' missing"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("encoded extract: {e}"))?;
        let shape = enc.shape(); // [1, D, T']
        let (d, tp) = (shape[1], shape[2]);
        let flat: Vec<f32> = enc.iter().copied().collect();

        let enc_len = out
            .get("encoded_len")
            .ok_or_else(|| anyhow!("encoder output 'encoded_len' missing"))?
            .try_extract_array::<i32>()
            .map_err(|e| anyhow!("encoded_len extract: {e}"))?
            .iter()
            .next()
            .copied()
            .unwrap_or(tp as i32)
            .max(0) as usize;

        Ok((flat, d, tp, enc_len.min(tp)))
    }

    /// One transducer step: prediction network (decoder) then joiner. Returns the joint
    /// logits over the vocab plus the new LSTM state.
    fn decode_step(
        &mut self,
        last_token: i64,
        h: &ArrayD<f32>,
        c: &ArrayD<f32>,
        enc_frame: Vec<f32>,
        enc_dim: usize,
    ) -> Result<(Vec<f32>, ArrayD<f32>, ArrayD<f32>)> {
        use ort::inputs;
        use ort::value::TensorRef;

        // Prediction network. Extract everything owned before the joiner call so the
        // `&mut self.decoder` borrow is released.
        let (dec, h_new, c_new) = {
            let x = Array2::from_shape_vec((1, 1), vec![last_token])
                .map_err(|e| anyhow!("dec x: {e}"))?;
            let x_ref =
                TensorRef::from_array_view(x.view()).map_err(|e| anyhow!("dec x ref: {e}"))?;
            let h_ref =
                TensorRef::from_array_view(h.view()).map_err(|e| anyhow!("dec h ref: {e}"))?;
            let c_ref =
                TensorRef::from_array_view(c.view()).map_err(|e| anyhow!("dec c ref: {e}"))?;
            let out = self
                .decoder
                .run(inputs!["x" => x_ref, "h.1" => h_ref, "c.1" => c_ref])
                .map_err(|e| anyhow!("decoder run: {e}"))?;
            let dec: Vec<f32> = out
                .get("dec")
                .ok_or_else(|| anyhow!("decoder output 'dec' missing"))?
                .try_extract_array::<f32>()
                .map_err(|e| anyhow!("dec extract: {e}"))?
                .iter()
                .copied()
                .collect();
            let h_new = out
                .get("h")
                .ok_or_else(|| anyhow!("decoder output 'h' missing"))?
                .try_extract_array::<f32>()
                .map_err(|e| anyhow!("h extract: {e}"))?
                .to_owned();
            let c_new = out
                .get("c")
                .ok_or_else(|| anyhow!("decoder output 'c' missing"))?
                .try_extract_array::<f32>()
                .map_err(|e| anyhow!("c extract: {e}"))?
                .to_owned();
            (dec, h_new, c_new)
        };

        // Joiner: enc frame [1,D,1] + prediction output reshaped to [1,320,1].
        let enc_in = Array3::from_shape_vec((1, enc_dim, 1), enc_frame)
            .map_err(|e| anyhow!("joiner enc: {e}"))?;
        let dec_in = Array3::from_shape_vec((1, PRED_HIDDEN, 1), dec)
            .map_err(|e| anyhow!("joiner dec: {e}"))?;
        let joint: Vec<f32> = {
            let e_ref = TensorRef::from_array_view(enc_in.view())
                .map_err(|e| anyhow!("joiner enc ref: {e}"))?;
            let d_ref = TensorRef::from_array_view(dec_in.view())
                .map_err(|e| anyhow!("joiner dec ref: {e}"))?;
            let out = self
                .joiner
                .run(inputs!["enc" => e_ref, "dec" => d_ref])
                .map_err(|e| anyhow!("joiner run: {e}"))?;
            out.get("joint")
                .ok_or_else(|| anyhow!("joiner output 'joint' missing"))?
                .try_extract_array::<f32>()
                .map_err(|e| anyhow!("joint extract: {e}"))?
                .iter()
                .copied()
                .collect()
        };

        Ok((joint, h_new, c_new))
    }
}

fn argmax(v: &[f32]) -> usize {
    v.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(pieces: &[&str]) -> Vec<String> {
        pieces.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pieces_group_into_words_with_frame_timing() {
        // "▁при вет ▁мир ." at frames 3,4,10,11 with 40 ms frames.
        let v = vocab(&["▁при", "вет", "▁мир", "."]);
        let words = tokens_to_timed_words(&v, &[0, 1, 2, 3], &[3, 4, 10, 11], 40.0);
        assert_eq!(
            words,
            vec![
                TimedWord {
                    text: "привет".into(),
                    start_ms: 120,
                    end_ms: 200
                },
                TimedWord {
                    text: "мир.".into(),
                    start_ms: 400,
                    end_ms: 480
                },
            ]
        );
    }

    #[test]
    fn leading_continuation_piece_still_forms_a_word() {
        // Audio cut mid-word: first piece has no ▁ marker.
        let v = vocab(&["вет", "▁мир"]);
        let words = tokens_to_timed_words(&v, &[0, 1], &[0, 5], 40.0);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "вет");
        assert_eq!(words[1].text, "мир");
    }

    #[test]
    fn bare_marker_and_unknown_ids_are_skipped() {
        let v = vocab(&["\u{2581}", "▁да"]);
        let words = tokens_to_timed_words(&v, &[0, 99, 1], &[0, 1, 2], 40.0);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "да");
    }

    // Research harness: word-level timestamps on a real clip. Env-gated:
    //   GIGAAM_RNNT_DIR=<model dir>  GIGAAM_TEST_WAV=<wav/mp4>  WINDOW_MS=<start,end>
    //   cargo test -p meetily --lib gigaam_engine::rnnt::tests::research_word_timestamps -- --ignored --nocapture
    #[test]
    #[ignore]
    fn research_word_timestamps() {
        let dir = match std::env::var("GIGAAM_RNNT_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => return,
        };
        let wav = std::env::var("GIGAAM_TEST_WAV").expect("set GIGAAM_TEST_WAV");
        let window = std::env::var("WINDOW_MS").unwrap_or_default();
        let (w_start, w_end) = match window.split_once(',') {
            Some((a, b)) => (
                a.trim().parse::<usize>().unwrap(),
                b.trim().parse::<usize>().unwrap(),
            ),
            None => (0, usize::MAX),
        };

        let mut model = RnntModel::load(
            &dir.join("v3_e2e_rnnt_encoder.onnx"),
            &dir.join("v3_e2e_rnnt_decoder.onnx"),
            &dir.join("v3_e2e_rnnt_joint.onnx"),
            &dir.join("v3_e2e_rnnt_vocab.txt"),
        )
        .expect("load RNN-T model");

        let decoded =
            crate::audio::decoder::decode_audio_file(std::path::Path::new(&wav)).expect("decode");
        let samples = decoded.to_whisper_format();
        let s = (w_start * 16).min(samples.len());
        let e = (w_end.saturating_mul(16)).min(samples.len());
        let words = model
            .transcribe_with_words(&samples[s..e])
            .expect("transcribe");
        for w in &words {
            eprintln!(
                "{:8.2}-{:8.2}s  {}",
                (w_start as f64 + w.start_ms as f64) / 1000.0,
                (w_start as f64 + w.end_ms as f64) / 1000.0,
                w.text
            );
        }
    }

    /// ONNX encoder vs Neural Engine encoder on the same audio: prints both transcripts and
    /// both encoder timings. fp16 on the ANE does not reproduce fp32 bit-for-bit, so the
    /// point is that the text is the same modulo fp16-level wobble (fillers, commas) — that
    /// is what the reference implementation observed too, and it is why this stays a
    /// human-read research test rather than an equality assertion.
    ///
    ///   GIGAAM_RNNT_DIR=<dir with v3_e2e_rnnt_{encoder,decoder,joint}.onnx + vocab> \
    ///   GIGAAM_ANE_MODEL=<path to encoder-ane.mlmodelc> \
    ///   GIGAAM_TEST_WAV=<wav/mp4>  WINDOW_MS=<start,end> \
    ///   cargo test --lib gigaam_engine::rnnt::tests::ane_matches_onnx -- --ignored --nocapture
    #[test]
    #[ignore]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn ane_matches_onnx() {
        let (Ok(dir), Ok(ane)) = (
            std::env::var("GIGAAM_RNNT_DIR"),
            std::env::var("GIGAAM_ANE_MODEL"),
        ) else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let wav = std::env::var("GIGAAM_TEST_WAV").expect("set GIGAAM_TEST_WAV");
        let (w_start, w_end) = match std::env::var("WINDOW_MS").unwrap_or_default().split_once(',') {
            Some((a, b)) => (
                a.trim().parse::<usize>().unwrap(),
                b.trim().parse::<usize>().unwrap(),
            ),
            None => (0, 30_000),
        };

        let decoded =
            crate::audio::decoder::decode_audio_file(std::path::Path::new(&wav)).expect("decode");
        let samples = decoded.to_whisper_format();
        let s = (w_start * 16).min(samples.len());
        let e = (w_end * 16).min(samples.len());
        let clip = &samples[s..e];
        eprintln!("clip: {:.1} s", clip.len() as f64 / 16_000.0);

        let mut onnx = RnntModel::load(
            &dir.join("v3_e2e_rnnt_encoder.onnx"),
            &dir.join("v3_e2e_rnnt_decoder.onnx"),
            &dir.join("v3_e2e_rnnt_joint.onnx"),
            &dir.join("v3_e2e_rnnt_vocab.txt"),
        )
        .expect("load ONNX RNN-T");
        let started = std::time::Instant::now();
        let onnx_text = onnx.transcribe(clip).expect("onnx transcribe");
        let onnx_ms = started.elapsed().as_millis();

        let mut ane = RnntModel::load_with_ane_encoder(
            std::path::Path::new(&ane),
            &dir.join("v3_e2e_rnnt_decoder.onnx"),
            &dir.join("v3_e2e_rnnt_joint.onnx"),
            &dir.join("v3_e2e_rnnt_vocab.txt"),
        )
        .expect("load ANE RNN-T");
        let started = std::time::Instant::now();
        let ane_text = ane.transcribe(clip).expect("ane transcribe");
        let ane_ms = started.elapsed().as_millis();

        eprintln!("\nONNX ({onnx_ms} ms):\n{onnx_text}\n\nANE ({ane_ms} ms):\n{ane_text}\n");
        assert!(!ane_text.trim().is_empty(), "ANE produced no transcript");
        // Same audio, same decoder: the transcripts must be recognizably the same text.
        let onnx_words = onnx_text.split_whitespace().count();
        let ane_words = ane_text.split_whitespace().count();
        assert!(
            ane_words * 10 >= onnx_words * 8 && onnx_words * 10 >= ane_words * 8,
            "word counts diverge too far: onnx {onnx_words} vs ane {ane_words}"
        );
    }

    /// End-to-end check against the real RNN-T ONNX files. Ignored by default; run with:
    ///   GIGAAM_RNNT_DIR=<dir with v3_e2e_rnnt_{encoder.int8,decoder.int8,joint.int8}.onnx + vocab>
    ///   GIGAAM_TEST_F32=<raw 16kHz mono f32le clip>
    ///   cargo test --lib gigaam_engine::rnnt -- --ignored --nocapture
    #[test]
    #[ignore]
    fn rnnt_transcribes_real_audio() {
        let dir = match std::env::var("GIGAAM_RNNT_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => return, // skip when not configured
        };
        let wav = std::env::var("GIGAAM_TEST_F32").expect("set GIGAAM_TEST_F32");

        let mut model = RnntModel::load(
            &dir.join("v3_e2e_rnnt_encoder.int8.onnx"),
            &dir.join("v3_e2e_rnnt_decoder.int8.onnx"),
            &dir.join("v3_e2e_rnnt_joint.int8.onnx"),
            &dir.join("v3_e2e_rnnt_vocab.txt"),
        )
        .expect("load RNN-T model");

        let bytes = std::fs::read(&wav).expect("read f32 clip");
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        let text = model.transcribe(&samples).expect("transcribe");
        println!("RNN-T transcript: {text:?}");
        assert!(!text.trim().is_empty(), "expected non-empty transcript");
    }
}
