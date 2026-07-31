//! GigaAM v3 encoder on the Apple Neural Engine (macOS, Apple Silicon only).
//!
//! The encoder is ~99% of the RNN-T pipeline's cost: on ONNX/CPU it pegs every performance
//! core for the whole transcription. CoreML runs the *same weights* as an fp16 MLProgram on
//! the ANE instead — model from [gigaam-v3-coreml], converted 1:1 from the
//! `istupakov/gigaam-v3-onnx` export this engine already uses. Only the encoder is
//! converted; the RNN-T prediction network and joiner stay on `ort` (7 MB of ONNX), so the
//! 885 MB fp32 encoder is never downloaded in this mode.
//!
//! Ported from GigaType2's `resources/macos-gigaam-encoder.swift`, minus its helper process:
//! that app is Electron, and CoreML is unreachable from a Node process, so it shells out to
//! a Swift sidecar over a stdio protocol. We are already a native process — `cidre` binds
//! CoreML directly, so the model lives in-process with no IPC and no extra signed binary.
//!
//! **Fixed shapes.** The ANE cannot do dynamic shapes, so the graph's input is always
//! `[1, 64, 3360]` (33.6 s of mel frames) and shorter requests are zero-padded. Two
//! consequences, both inherited from the reference implementation:
//!   - the graph's own `encoded_len` is one too high on padded input, so the valid output
//!     length is recomputed as `ceil(frames / 4)` — the same subsampling rule `ort` reports;
//!   - the ANE always pays for the full window, so below ~5 s of audio ONNX is slightly
//!     faster. Everywhere above that it wins by a lot (measured on an M4, 30 s of speech:
//!     encoder 115 ms vs 1200 ms on ONNX CPU, peak RSS 191 MB vs 1236 MB).
//!
//! [gigaam-v3-coreml]: https://github.com/IsaacClarke2/gigaam-v3-coreml
//!
//! Env overrides (debugging): `GIGAAM_ANE_COMPUTE_UNITS=cpu_ane|all|cpu`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use cidre::{arc, ml, ns};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

use super::featurizer::{Featurizer, HOP, N_MELS, WIN_LEN};

/// Mel frames per prediction — fixed by the CoreML graph (`audio_signal` is `[1,64,3360]`).
pub const WINDOW_FRAMES: usize = 3360;

/// Encoder time subsampling (T → ceil(T/4)), matching the ONNX export.
const SUBSAMPLE: usize = 4;

/// Longest waveform a single prediction can cover (33.6 s at 16 kHz).
pub const WINDOW_SAMPLES: usize = (WINDOW_FRAMES - 1) * HOP + WIN_LEN;

/// Compiled-model directory inside the GigaAM model dir. CoreML consumes an `.mlmodelc`;
/// the download is an `.mlpackage`, compiled once by [`compile_model`].
pub const COMPILED_DIR_NAME: &str = "encoder-ane.mlmodelc";

/// GigaAM v3 encoder as a CoreML model pinned to CPU + Neural Engine.
///
/// One model, one reusable input buffer, one prediction at a time — the caller (the global
/// engine mutex in [`super`]) serializes access.
pub struct AneEncoder {
    model: arc::R<ml::Model>,
    /// `audio_signal` f32 `[1, 64, 3360]`, refilled per request (zero-padded tail).
    audio_signal: arc::R<ml::MultiArray>,
    /// `length` i32 `[1]` — the number of *valid* mel frames in `audio_signal`.
    length: arc::R<ml::MultiArray>,
    audio_signal_key: arc::R<ns::String>,
    length_key: arc::R<ns::String>,
    encoded_key: arc::R<ns::String>,
    /// Encoder feature dim (768 for GigaAM v3), learned during warmup.
    enc_dim: usize,
}

// SAFETY: `MLModel` predictions are documented as thread-safe, and every access here goes
// through the process-wide engine mutex anyway (see `super::ENGINE`), so the retained
// CoreML objects are never touched from two threads at once. Send is needed because the
// loaded model lives in a `static Mutex<Option<LoadedModel>>`.
unsafe impl Send for AneEncoder {}

impl AneEncoder {
    /// Load a compiled `.mlmodelc` and warm it up.
    ///
    /// The first load on a given machine pays a one-time ANE specialization (tens of
    /// seconds, cached by the OS afterwards); the warmup pass below is where that happens,
    /// so it lands during model load rather than inside the first transcription.
    pub fn load(model_dir: &Path) -> Result<Self> {
        if !is_compiled_model_usable(model_dir) {
            bail!(
                "CoreML model at {} is missing or incomplete (expected a compiled .mlmodelc)",
                model_dir.display()
            );
        }
        let path = model_dir
            .to_str()
            .ok_or_else(|| anyhow!("model path is not valid UTF-8: {}", model_dir.display()))?;

        let started = Instant::now();
        let url = ns::Url::with_fs_path_str(path, true);
        let mut cfg = ml::ModelCfg::new();
        // `cidre` keeps `MLComputeUnits` in a private module, so the enum itself can't be
        // named here — the raw value goes through `objc_msgSend` instead (see
        // [`compute_units_raw`]). The configuration object stays `cidre`-managed.
        unsafe {
            let cfg_ptr: *mut ml::ModelCfg = &mut *cfg;
            let _: () = msg_send![cfg_ptr as *mut Object, setComputeUnits: compute_units_raw()];
        }
        let model = ml::Model::with_cfg(&url, &cfg).map_err(|e| {
            anyhow!(
                "CoreML load failed ({}). The Neural Engine encoder needs macOS 14 or newer.",
                describe(e)
            )
        })?;
        let load_ms = started.elapsed().as_millis();

        let audio_signal = new_multi_array(
            &[1, N_MELS as isize, WINDOW_FRAMES as isize],
            ml::MultiArrayDType::F32,
        )?;
        let length = new_multi_array(&[1], ml::MultiArrayDType::I32)?;

        let mut encoder = Self {
            model,
            audio_signal,
            length,
            audio_signal_key: ns::String::with_str("audio_signal"),
            length_key: ns::String::with_str("length"),
            encoded_key: ns::String::with_str("encoded"),
            enc_dim: 0,
        };

        let started = Instant::now();
        let (_, dim, _) = encoder.predict(&vec![0f32; N_MELS * WINDOW_FRAMES], WINDOW_FRAMES)?;
        encoder.enc_dim = dim;
        log::info!(
            "GigaAM ANE encoder ready: dim {dim}, window {WINDOW_FRAMES} frames, \
             load {load_ms} ms, warmup {} ms, compute units {}",
            started.elapsed().as_millis(),
            compute_units_label(),
        );
        Ok(encoder)
    }

    /// Encoder output dim (768 for GigaAM v3).
    pub fn enc_dim(&self) -> usize {
        self.enc_dim
    }

    /// Encode a whole mel sequence, chunking it into [`WINDOW_FRAMES`] windows when needed.
    ///
    /// `features` is row-major `[N_MELS][frames]` (the featurizer's layout). Returns the
    /// encoder output flattened row-major `[dim][out_frames]` plus `(dim, out_frames)` —
    /// the same shape contract the ONNX path in [`super::rnnt`] produces, so the transducer
    /// decode is identical either way.
    ///
    /// Sequences longer than one window are encoded window by window and concatenated along
    /// time. That clips the conformer's context at the seams, so callers should keep spans
    /// inside a single window when they can (every current caller caps segments at 25 s).
    pub fn encode_sequence(
        &mut self,
        features: &[f32],
        frames: usize,
    ) -> Result<(Vec<f32>, usize, usize)> {
        if frames == 0 {
            return Ok((Vec::new(), self.enc_dim, 0));
        }
        if features.len() != N_MELS * frames {
            bail!(
                "feature buffer is {} floats, expected {} ({N_MELS} mels × {frames} frames)",
                features.len(),
                N_MELS * frames
            );
        }
        if frames <= WINDOW_FRAMES {
            return self.predict(features, frames);
        }

        log::debug!(
            "GigaAM ANE: {frames} mel frames exceed the {WINDOW_FRAMES}-frame window — \
             encoding in {} chunks",
            frames.div_ceil(WINDOW_FRAMES)
        );

        // Encode each window, then stitch the per-window `[dim][t]` blocks into one
        // `[dim][Σt]` block (row-major, so each dim's row is contiguous).
        let mut chunks: Vec<(Vec<f32>, usize, usize)> = Vec::new();
        let mut start = 0usize;
        while start < frames {
            let len = (frames - start).min(WINDOW_FRAMES);
            let mut window = vec![0f32; N_MELS * len];
            for mel in 0..N_MELS {
                let src = mel * frames + start;
                window[mel * len..(mel + 1) * len].copy_from_slice(&features[src..src + len]);
            }
            chunks.push(self.predict(&window, len)?);
            start += len;
        }

        let dim = chunks[0].1;
        let total: usize = chunks.iter().map(|(_, _, t)| t).sum();
        let mut out = vec![0f32; dim * total];
        let mut offset = 0usize;
        for (data, chunk_dim, chunk_frames) in &chunks {
            if *chunk_dim != dim {
                bail!("encoder returned inconsistent dims across windows ({dim} vs {chunk_dim})");
            }
            for d in 0..dim {
                let src = d * chunk_frames;
                let dst = d * total + offset;
                out[dst..dst + chunk_frames].copy_from_slice(&data[src..src + chunk_frames]);
            }
            offset += chunk_frames;
        }
        Ok((out, dim, total))
    }

    /// One prediction over a single window. `features` is row-major `[N_MELS][frames]` with
    /// `frames <= WINDOW_FRAMES`; the rest of the window is zero-padded.
    ///
    /// Everything runs inside an autorelease pool: CoreML hands back autoreleased objects
    /// (the output provider, the output array and its ~2.5 MB backing store), and a Rust
    /// thread — a Tokio blocking worker here — has no pool of its own to drain them, so they
    /// would accumulate for the life of the thread.
    fn predict(&mut self, features: &[f32], frames: usize) -> Result<(Vec<f32>, usize, usize)> {
        if frames == 0 || frames > WINDOW_FRAMES {
            bail!("frames must be 1..={WINDOW_FRAMES} (got {frames})");
        }

        write_features(&mut self.audio_signal, features, frames);
        write_length(&mut self.length, frames as i32);

        objc::rc::autoreleasepool(|| {
            // MLDictionaryFeatureProvider is the one piece of CoreML `cidre` doesn't bind, so
            // it goes through a raw `objc_msgSend`. It accepts MLMultiArray values directly
            // and wraps them in MLFeatureValues itself.
            let dict = ns::Dictionary::with_keys_values(
                &[&*self.audio_signal_key, &*self.length_key],
                &[&*self.audio_signal, &*self.length],
            );
            let provider = DictionaryFeatureProvider::new(&dict)?;

            let output = self
                .model
                .prediction_from_features(provider.as_feature_provider())
                .map_err(|e| anyhow!("CoreML prediction failed: {}", describe(e)))?;

            // `read_encoded` copies the samples out before the pool drains.
            read_encoded(&output, &self.encoded_key, frames)
        })
    }
}

/// Compile a downloaded `.mlpackage` into `dest` (an `encoder-ane.mlmodelc` directory).
///
/// Uses CoreML's own compiler through `MLModel.compileModel(at:)`, which ships with the OS
/// — unlike `xcrun coremlcompiler` (what GigaType2 runs at *build* time), which needs full
/// Xcode. That is why the model is compiled on the user's machine here: the download stays
/// a plain archive and no Xcode is required anywhere.
///
/// CoreML writes the result into a temporary directory, so it is moved into place after
/// compilation; `dest` is replaced atomically enough for our purposes (a half-written
/// `.mlmodelc` is worse than none — CoreML then fails at load time).
pub fn compile_model(mlpackage: &Path, dest: &Path) -> Result<()> {
    let source = mlpackage
        .to_str()
        .ok_or_else(|| anyhow!("model path is not valid UTF-8: {}", mlpackage.display()))?;
    if !mlpackage.exists() {
        bail!("no .mlpackage at {}", mlpackage.display());
    }

    let started = Instant::now();
    let compiled = unsafe {
        let url = ns::Url::with_fs_path_str(source, true);
        let url_ptr: *const ns::Url = &*url;
        let mut error: *mut Object = std::ptr::null_mut();
        let compiled: *mut Object = msg_send![
            class!(MLModel),
            compileModelAtURL: url_ptr as *mut Object
            error: &mut error
        ];
        if compiled.is_null() {
            bail!("CoreML compileModel failed: {}", describe_objc_error(error));
        }
        // The returned URL is autoreleased (+0) and only needed for its path.
        let url: &ns::Url = &*(compiled as *const ns::Url);
        url.path()
            .map(|p| p.as_cf().to_string())
            .ok_or_else(|| anyhow!("CoreML compileModel returned a URL with no path"))?
    };

    let compiled = PathBuf::from(compiled);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging = dest.with_extension("compiling");
    let _ = std::fs::remove_dir_all(&staging);
    // Same volume in practice (temp dir and app data are both on the data volume), but a
    // cross-device rename fails with EXDEV, so fall back to a recursive copy.
    if std::fs::rename(&compiled, &staging).is_err() {
        copy_dir(&compiled, &staging)?;
        let _ = std::fs::remove_dir_all(&compiled);
    }
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&staging, dest)?;

    if !is_compiled_model_usable(dest) {
        bail!(
            "compiled model at {} is missing coremldata.bin/model.mil/weights",
            dest.display()
        );
    }
    log::info!(
        "GigaAM ANE encoder compiled in {} ms → {}",
        started.elapsed().as_millis(),
        dest.display()
    );
    Ok(())
}

/// A compiled model is a directory; treat it as present only when CoreML's own manifest, the
/// MLProgram body and the weights are all there. A half-written `.mlmodelc` otherwise fails
/// deep inside model load, long after the download looked successful.
///
/// Deliberately does not look for `metadata.json`: `xcrun coremlcompiler` writes one (so
/// build-time-compiled bundles like GigaType2's have it), but CoreML's runtime compiler —
/// what [`compile_model`] uses — does not.
pub fn is_compiled_model_usable(dir: &Path) -> bool {
    dir.join("coremldata.bin").exists()
        && dir.join("model.mil").exists()
        && dir.join("weights").exists()
}

/// Mel frames the featurizer produces for `samples` — used to decide whether an ASR span
/// fits one ANE window.
pub fn frames_for_samples(samples: usize) -> usize {
    Featurizer::num_frames(samples)
}

/// `MLComputeUnits` raw value for the model, overridable with `GIGAAM_ANE_COMPUTE_UNITS`:
/// `MLComputeUnitsCPUOnly = 0`, `CPUAndGPU = 1`, `All = 2`, `CPUAndNeuralEngine = 3`.
///
/// CPU + Neural Engine (not `All`) is deliberate: letting CoreML hand parts of the graph to
/// the GPU costs more than it saves here, and `cpu_ane` is what the reference ships.
fn compute_units_raw() -> isize {
    match std::env::var("GIGAAM_ANE_COMPUTE_UNITS").as_deref() {
        Ok("cpu") => 0,
        Ok("all") => 2,
        _ => 3,
    }
}

fn compute_units_label() -> &'static str {
    match compute_units_raw() {
        0 => "cpu",
        2 => "all",
        _ => "cpu_ane",
    }
}

fn new_multi_array(shape: &[isize], d_type: ml::MultiArrayDType) -> Result<arc::R<ml::MultiArray>> {
    let numbers: Vec<arc::R<ns::Number>> = shape
        .iter()
        .map(|d| ns::Number::with_i32(*d as i32))
        .collect();
    let shape = ns::Array::from_slice_retained(&numbers);
    ml::MultiArray::with_shape(&shape, d_type)
        .map_err(|e| anyhow!("MLMultiArray allocation failed: {}", describe(e)))
}

/// Fill `audio_signal` with `frames` columns of `features` (row-major `[mel][frame]`) and
/// zero the padded tail. CoreML does not promise dense packing, so writes go through the
/// strides it reports.
fn write_features(array: &mut ml::MultiArray, features: &[f32], frames: usize) {
    array.bytes_mut(|ptr, size, strides| {
        let base = ptr as *mut f32;
        let floats = (size as usize) / std::mem::size_of::<f32>();
        unsafe { std::ptr::write_bytes(base, 0, floats) };
        let mel_stride = stride_at(strides, 1, WINDOW_FRAMES);
        let frame_stride = stride_at(strides, 2, 1);
        for mel in 0..N_MELS {
            let src = mel * frames;
            let dst = mel * mel_stride;
            for frame in 0..frames {
                unsafe { *base.add(dst + frame * frame_stride) = features[src + frame] };
            }
        }
    });
}

fn write_length(array: &mut ml::MultiArray, frames: i32) {
    array.bytes_mut(|ptr, _size, _strides| unsafe {
        *(ptr as *mut i32) = frames;
    });
}

/// Read the `encoded` output as row-major `[dim][valid]`.
///
/// `encoded_len` from the graph is one too high on zero-padded input (the reference
/// implementation hit the same thing), so the valid length is `ceil(frames / 4)` — `ort`'s
/// subsampling rule — clamped to what the fixed-shape output actually holds.
fn read_encoded(
    output: &ml::AnyFeatureProvider,
    key: &ns::String,
    frames: usize,
) -> Result<(Vec<f32>, usize, usize)> {
    use cidre::ml::FeatureProvider;

    let value = output
        .feature_value_for_name(key)
        .ok_or_else(|| anyhow!("CoreML prediction has no 'encoded' output"))?;
    // `MLFeatureValue.multiArrayValue` is the other binding `cidre` lacks. The result is
    // autoreleased (+0) and consumed before this scope ends.
    let array: &ml::MultiArray = unsafe {
        let value_ptr: *const ml::FeatureValue = &*value;
        let array: *mut Object = msg_send![value_ptr as *mut Object, multiArrayValue];
        if array.is_null() {
            bail!("'encoded' output is not a multi-array");
        }
        &*(array as *const ml::MultiArray)
    };

    let shape = array.shape(); // [1, dim, T']
    if shape.len() < 3 {
        bail!("'encoded' has {} dims, expected 3", shape.len());
    }
    let dim = number_at(&shape, 1, 0);
    let available = number_at(&shape, 2, 0);
    if dim == 0 || available == 0 {
        bail!("'encoded' has an empty shape [1, {dim}, {available}]");
    }
    let valid = frames.div_ceil(SUBSAMPLE).min(available);

    let strides = array.strides();
    let dim_stride = stride_at(&strides, 1, available);
    let frame_stride = stride_at(&strides, 2, 1);

    let mut out = vec![0f32; dim * valid];
    array.bytes(|ptr, _size| {
        let base = ptr as *const f32;
        for d in 0..dim {
            let src = d * dim_stride;
            let dst = d * valid;
            for t in 0..valid {
                out[dst + t] = unsafe { *base.add(src + t * frame_stride) };
            }
        }
    });
    Ok((out, dim, valid))
}

/// `MLDictionaryFeatureProvider`, retained for the duration of one prediction.
struct DictionaryFeatureProvider(*mut Object);

impl DictionaryFeatureProvider {
    fn new(dict: &ns::Dictionary<ns::String, ml::MultiArray>) -> Result<Self> {
        unsafe {
            let dict_ptr: *const ns::Dictionary<ns::String, ml::MultiArray> = dict;
            let allocated: *mut Object = msg_send![class!(MLDictionaryFeatureProvider), alloc];
            let mut error: *mut Object = std::ptr::null_mut();
            let provider: *mut Object = msg_send![
                allocated,
                initWithDictionary: dict_ptr as *mut Object
                error: &mut error
            ];
            if provider.is_null() {
                bail!(
                    "MLDictionaryFeatureProvider init failed: {}",
                    describe_objc_error(error)
                );
            }
            Ok(Self(provider))
        }
    }

    fn as_feature_provider(&self) -> &ml::AnyFeatureProvider {
        unsafe { &*(self.0 as *const ml::AnyFeatureProvider) }
    }
}

impl Drop for DictionaryFeatureProvider {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.0, release];
        }
    }
}

fn stride_at(strides: &ns::Array<ns::Number>, index: usize, fallback: usize) -> usize {
    number_at(strides, index, fallback)
}

fn number_at(array: &ns::Array<ns::Number>, index: usize, fallback: usize) -> usize {
    match array.get(index) {
        Ok(number) => {
            let value = number.as_isize();
            if value > 0 {
                value as usize
            } else {
                fallback
            }
        }
        Err(_) => fallback,
    }
}

fn describe(error: &ns::Error) -> String {
    error.localized_description().as_cf().to_string()
}

/// `NSError.localizedDescription` for an error reached through a raw `objc_msgSend`.
fn describe_objc_error(error: *mut Object) -> String {
    if error.is_null() {
        return "unknown error".to_string();
    }
    let error: &ns::Error = unsafe { &*(error as *const ns::Error) };
    describe(error)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_covers_33_6_seconds() {
        // The reference implementation reports 537,760 max samples for its 3360-frame window.
        assert_eq!(WINDOW_SAMPLES, 537_760);
        assert_eq!(Featurizer::num_frames(WINDOW_SAMPLES), WINDOW_FRAMES);
        // One more sample must not fit — that is what makes `encode_sequence` chunk.
        assert!(Featurizer::num_frames(WINDOW_SAMPLES + HOP) > WINDOW_FRAMES);
    }

    /// Repeated full-window encodes must not grow the process — that is what the autorelease
    /// pool in [`AneEncoder::predict`] is for (each prediction hands back a ~2.5 MB
    /// autoreleased output array). Ignored; run with:
    ///   GIGAAM_ANE_MODEL=<path to encoder-ane.mlmodelc> \
    ///   cargo test --lib gigaam_engine::coreml::tests::encoding_does_not_leak -- --ignored --nocapture
    #[test]
    #[ignore]
    fn encoding_does_not_leak() {
        let Ok(dir) = std::env::var("GIGAAM_ANE_MODEL") else {
            return;
        };
        let mut encoder = AneEncoder::load(Path::new(&dir)).expect("load ANE encoder");
        let window = vec![0f32; N_MELS * WINDOW_FRAMES];

        encoder
            .encode_sequence(&window, WINDOW_FRAMES)
            .expect("warm encode");
        let baseline = memory_stats::memory_stats().expect("rss").physical_mem;
        for _ in 0..20 {
            encoder
                .encode_sequence(&window, WINDOW_FRAMES)
                .expect("encode");
        }
        let after = memory_stats::memory_stats().expect("rss").physical_mem;
        let growth = after.saturating_sub(baseline);
        eprintln!(
            "RSS {} MB → {} MB over 20 encodes (+{} MB)",
            baseline / 1024 / 1024,
            after / 1024 / 1024,
            growth / 1024 / 1024
        );
        // Without a pool this grows by ~2.5 MB per prediction; the allowance covers CoreML's
        // own caches without letting a real leak through.
        assert!(
            growth < 24 * 1024 * 1024,
            "RSS grew by {} MB over 20 encodes",
            growth / 1024 / 1024
        );
    }

    /// Compile a real `.mlpackage` the way an install does, then load and run the result —
    /// this is the one step that only happens on a user's machine. Ignored; run with:
    ///   GIGAAM_ANE_MLPACKAGE=<path to *.mlpackage> \
    ///   cargo test --lib gigaam_engine::coreml::tests::compiles_and_loads_mlpackage -- --ignored --nocapture
    #[test]
    #[ignore]
    fn compiles_and_loads_mlpackage() {
        let Ok(package) = std::env::var("GIGAAM_ANE_MLPACKAGE") else {
            return;
        };
        let work = std::env::temp_dir().join("meetily-ane-compile-test");
        let dest = work.join(COMPILED_DIR_NAME);
        let _ = std::fs::remove_dir_all(&work);

        compile_model(Path::new(&package), &dest).expect("compile .mlpackage");
        assert!(
            is_compiled_model_usable(&dest),
            "compiled model is incomplete"
        );

        let mut encoder = AneEncoder::load(&dest).expect("load the freshly compiled model");
        let frames = 100;
        let (encoded, dim, out_frames) = encoder
            .encode_sequence(&vec![0f32; N_MELS * frames], frames)
            .expect("encode");
        assert_eq!(dim, 768);
        assert_eq!(out_frames, frames.div_ceil(SUBSAMPLE));
        assert_eq!(encoded.len(), dim * out_frames);

        let _ = std::fs::remove_dir_all(&work);
    }

    /// Smoke test against a real compiled model. Ignored by default; run with:
    ///   GIGAAM_ANE_MODEL=<path to encoder-ane.mlmodelc> \
    ///   cargo test --lib gigaam_engine::coreml::tests::ane_encodes_a_tone -- --ignored --nocapture
    #[test]
    #[ignore]
    fn ane_encodes_a_tone() {
        let Ok(dir) = std::env::var("GIGAAM_ANE_MODEL") else {
            return;
        };
        let mut encoder = AneEncoder::load(Path::new(&dir)).expect("load ANE encoder");

        let waveform: Vec<f32> = (0..5 * 16_000)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        let (feats, frames) = Featurizer::new().compute(&waveform);

        let started = Instant::now();
        let (encoded, dim, out_frames) = encoder.encode_sequence(&feats, frames).expect("encode");
        eprintln!(
            "frames {frames} → dim {dim} × {out_frames} in {} ms",
            started.elapsed().as_millis()
        );
        assert_eq!(dim, 768, "GigaAM v3 encoder dim");
        assert_eq!(out_frames, frames.div_ceil(SUBSAMPLE));
        assert_eq!(encoded.len(), dim * out_frames);
        assert!(
            encoded.iter().any(|v| v.abs() > 1e-6),
            "encoder output is all zeros"
        );
    }
}
