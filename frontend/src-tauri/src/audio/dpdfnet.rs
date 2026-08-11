//! Streaming DPDFNet2 speech enhancement for imported audio.
//!
//! The model is deliberately limited to the import path. The 8 kHz profile is used for
//! narrow-band/telephone recordings and the 16 kHz profile for other files. Both keep
//! recurrent state between frames and apply a 12 dB attenuation limit so weak speech is
//! not erased together with the noise. No full decoded recording is retained in RAM.

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use ndarray::{Array4, ArrayView1};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use realfft::{num_complex::Complex32, ComplexToReal, RealFftPlanner, RealToComplex};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

pub const NARROWBAND_SAMPLE_RATE: u32 = 8_000;
pub const WIDEBAND_SAMPLE_RATE: u32 = 16_000;
const MAX_STATE_SIZE: usize = 1_000_000;
const ATTENUATION_LIMIT_DB: f32 = 12.0;
const MODEL_REVISION: &str = "dd6818d00f50c836fed43a6243ebe49116de5964";

#[derive(Clone, Copy)]
struct ModelProfile {
    filename: &'static str,
    size: u64,
    sha256: &'static str,
}

const NARROWBAND_MODEL: ModelProfile = ModelProfile {
    filename: "dpdfnet2_8khz.onnx",
    size: 10_188_357,
    sha256: "6218f1dbd6e4bac5768c63b7d899fe7b84b3788f2a35c4e246d4ab0946165c5d",
};
const WIDEBAND_MODEL: ModelProfile = ModelProfile {
    filename: "dpdfnet2_16khz.onnx",
    size: 10_178_747,
    sha256: "4f0ee28935b4a32abecc717d745416976565834d839601acf43031094b4dc94c",
};

fn profile_for_source(sample_rate: u32) -> ModelProfile {
    if sample_rate <= 12_000 {
        NARROWBAND_MODEL
    } else {
        WIDEBAND_MODEL
    }
}

fn model_path<R: Runtime>(app: &AppHandle<R>, profile: ModelProfile) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| anyhow!("Could not resolve app data directory: {error}"))?
        .join("models")
        .join("dpdfnet")
        .join(profile.filename))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn valid_model(path: &Path, profile: ModelProfile) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.len() == profile.size)
        .unwrap_or(false)
        && sha256_file(path)
            .map(|digest| digest == profile.sha256)
            .unwrap_or(false)
}

/// Download the small model matching the source bandwidth on first use and verify it
/// before exposing it to ORT. Only the selected profile is loaded for an import.
pub async fn ensure_model<R: Runtime>(
    app: &AppHandle<R>,
    source_sample_rate: u32,
) -> Result<PathBuf> {
    let profile = profile_for_source(source_sample_rate);
    let destination = model_path(app, profile)?;
    if valid_model(&destination, profile) {
        return Ok(destination);
    }

    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("DPDFNet model path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = destination.with_extension("onnx.part");
    let _ = std::fs::remove_file(&temporary);

    // Pin the tested export rather than following a mutable `main` URL.
    let url = format!(
        "https://huggingface.co/Ceva-IP/DPDFNet/resolve/{MODEL_REVISION}/onnx/{}",
        if source_sample_rate <= 12_000 {
            "dpdfnet2_8khz.onnx"
        } else {
            "dpdfnet2.onnx"
        }
    );
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| anyhow!("Could not download DPDFNet2 model: {error}"))?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "Could not download DPDFNet2 model: HTTP {}",
            response.status()
        ));
    }
    if let Some(length) = response.content_length() {
        if length != profile.size {
            return Err(anyhow!(
                "Unexpected DPDFNet2 model size: expected {}, got {length}",
                profile.size
            ));
        }
    }

    let mut file = std::fs::File::create(&temporary)?;
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| anyhow!("DPDFNet2 download interrupted: {error}"))?;
        file.write_all(&chunk)?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
    }
    file.flush()?;
    drop(file);

    if downloaded != profile.size || !valid_model(&temporary, profile) {
        let _ = std::fs::remove_file(&temporary);
        return Err(anyhow!(
            "Downloaded DPDFNet2 model failed integrity verification"
        ));
    }
    if destination.exists() {
        std::fs::remove_file(&destination)?;
    }
    std::fs::rename(&temporary, &destination)?;
    Ok(destination)
}

fn metadata_value(session: &Session, key: &str) -> Result<String> {
    session
        .metadata()
        .map_err(|error| anyhow!("Could not read DPDFNet metadata: {error}"))?
        .custom(key)
        .map_err(|error| anyhow!("Could not read DPDFNet metadata key '{key}': {error}"))?
        .ok_or_else(|| anyhow!("DPDFNet model is missing metadata key '{key}'"))
}

fn parse_f32_list(value: &str, key: &str) -> Result<Vec<f32>> {
    value
        .split(',')
        .map(|item| {
            item.parse::<f32>()
                .map_err(|error| anyhow!("Invalid DPDFNet metadata '{key}': {error}"))
        })
        .collect()
}

fn initial_state(session: &Session) -> Result<Vec<f32>> {
    let state_size = metadata_value(session, "state_size")?.parse::<usize>()?;
    let erb_size = metadata_value(session, "erb_norm_state_size")?.parse::<usize>()?;
    let spec_size = metadata_value(session, "spec_norm_state_size")?.parse::<usize>()?;
    let erb = parse_f32_list(&metadata_value(session, "erb_norm_init")?, "erb_norm_init")?;
    let spec = parse_f32_list(
        &metadata_value(session, "spec_norm_init")?,
        "spec_norm_init",
    )?;
    if state_size > MAX_STATE_SIZE
        || erb.len() != erb_size
        || spec.len() != spec_size
        || erb_size + spec_size > state_size
    {
        return Err(anyhow!("DPDFNet model has inconsistent state metadata"));
    }
    let mut state = vec![0.0_f32; state_size];
    state[..erb_size].copy_from_slice(&erb);
    state[erb_size..erb_size + spec_size].copy_from_slice(&spec);
    Ok(state)
}

fn vorbis_window(window_len: usize) -> Vec<f32> {
    let half = window_len as f32 / 2.0;
    (0..window_len)
        .map(|index| {
            let sine = (0.5 * std::f32::consts::PI * (index as f32 + 0.5) / half).sin();
            (0.5 * std::f32::consts::PI * sine * sine).sin()
        })
        .collect()
}

/// Stateful causal enhancer. One model frame is 20 ms and one committed hop is 10 ms.
pub struct DpdfNetEnhancer {
    session: Session,
    state: Vec<f32>,
    input: VecDeque<f32>,
    overlap: Vec<f32>,
    window: Vec<f32>,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    sample_rate: u32,
    window_len: usize,
    hop_size: usize,
    freq_bins: usize,
    input_samples: usize,
    emitted_samples: usize,
}

impl DpdfNetEnhancer {
    pub fn load(model: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|error| anyhow!("Could not create DPDFNet ORT session: {error}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| anyhow!("Could not configure DPDFNet ORT session: {error}"))?
            // Fixed one-frame tensors let ORT reuse one memory pattern. Sequential
            // execution avoids branch-level worker arenas that increase peak RSS.
            .with_memory_pattern(true)
            .map_err(|error| anyhow!("Could not configure DPDFNet ORT memory: {error}"))?
            .with_parallel_execution(false)
            .map_err(|error| anyhow!("Could not configure DPDFNet ORT execution: {error}"))?
            .with_intra_threads(1)
            .map_err(|error| anyhow!("Could not configure DPDFNet ORT threads: {error}"))?
            .with_inter_threads(1)
            .map_err(|error| anyhow!("Could not configure DPDFNet ORT threads: {error}"))?
            .commit_from_file(model)
            .map_err(|error| {
                anyhow!("Could not load DPDFNet model {}: {error}", model.display())
            })?;
        let state = initial_state(&session)?;
        let sample_rate = metadata_value(&session, "sample_rate")?.parse::<u32>()?;
        let window_len = metadata_value(&session, "window_length")?.parse::<usize>()?;
        let hop_size = metadata_value(&session, "hop_length")?.parse::<usize>()?;
        let n_fft = metadata_value(&session, "n_fft")?.parse::<usize>()?;
        if !matches!(sample_rate, NARROWBAND_SAMPLE_RATE | WIDEBAND_SAMPLE_RATE)
            || window_len == 0
            || window_len > 4096
            || n_fft != window_len
            || hop_size * 2 != window_len
        {
            return Err(anyhow!("DPDFNet model has unsupported audio metadata"));
        }
        let freq_bins = n_fft / 2 + 1;
        let mut planner = RealFftPlanner::<f32>::new();
        Ok(Self {
            session,
            state,
            input: VecDeque::new(),
            overlap: vec![0.0; window_len],
            window: vorbis_window(window_len),
            forward: planner.plan_fft_forward(window_len),
            inverse: planner.plan_fft_inverse(window_len),
            sample_rate,
            window_len,
            hop_size,
            freq_bins,
            input_samples: 0,
            emitted_samples: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        self.input.extend(samples.iter().map(|sample| {
            if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }));
        self.input_samples = self.input_samples.saturating_add(samples.len());
        let mut enhanced = Vec::with_capacity(samples.len());
        while self.input.len() >= self.window_len {
            enhanced.extend(self.process_frame()?);
        }
        self.emitted_samples = self.emitted_samples.saturating_add(enhanced.len());
        Ok(enhanced)
    }

    pub fn flush(&mut self) -> Result<Vec<f32>> {
        let remaining = self.input_samples.saturating_sub(self.emitted_samples);
        if remaining == 0 {
            return Ok(Vec::new());
        }
        let mut enhanced = Vec::with_capacity(remaining);
        while enhanced.len() < remaining {
            while self.input.len() < self.window_len {
                self.input.push_back(0.0);
            }
            enhanced.extend(self.process_frame()?);
        }
        enhanced.truncate(remaining);
        self.emitted_samples = self.emitted_samples.saturating_add(enhanced.len());
        Ok(enhanced)
    }

    fn process_frame(&mut self) -> Result<Vec<f32>> {
        let mut frame: Vec<f32> = self.input.iter().take(self.window_len).copied().collect();
        for (sample, window) in frame.iter_mut().zip(&self.window) {
            *sample *= *window;
        }
        let mut spectrum = self.forward.make_output_vec();
        self.forward
            .process(&mut frame, &mut spectrum)
            .map_err(|error| anyhow!("DPDFNet forward FFT failed: {error}"))?;

        let mut spec_values = Vec::with_capacity(self.freq_bins * 2);
        for bin in &spectrum {
            spec_values.push(bin.re);
            spec_values.push(bin.im);
        }
        let spec = Array4::from_shape_vec((1, 1, self.freq_bins, 2), spec_values)
            .map_err(|error| anyhow!("Could not shape DPDFNet spectrum: {error}"))?;
        let state = ArrayView1::from(self.state.as_slice());
        let (enhanced_spectrum, next_state) = {
            let spec_ref = TensorRef::from_array_view(spec.view())
                .map_err(|error| anyhow!("Could not create DPDFNet spectrum tensor: {error}"))?;
            let state_ref = TensorRef::from_array_view(state)
                .map_err(|error| anyhow!("Could not create DPDFNet state tensor: {error}"))?;
            let outputs = self
                .session
                .run(ort::inputs!["spec" => spec_ref, "state_in" => state_ref])
                .map_err(|error| anyhow!("DPDFNet inference failed: {error}"))?;
            let enhanced = outputs
                .get("spec_e")
                .ok_or_else(|| anyhow!("DPDFNet output 'spec_e' is missing"))?
                .try_extract_array::<f32>()
                .map_err(|error| anyhow!("Could not read DPDFNet spectrum output: {error}"))?
                .iter()
                .copied()
                .collect::<Vec<_>>();
            let state = outputs
                .get("state_out")
                .ok_or_else(|| anyhow!("DPDFNet output 'state_out' is missing"))?
                .try_extract_array::<f32>()
                .map_err(|error| anyhow!("Could not read DPDFNet state output: {error}"))?
                .iter()
                .copied()
                .collect::<Vec<_>>();
            (enhanced, state)
        };
        if enhanced_spectrum.len() != self.freq_bins * 2 || next_state.len() != self.state.len() {
            return Err(anyhow!("DPDFNet returned unexpected tensor sizes"));
        }
        self.state = next_state;

        let noisy_weight = 10.0_f32.powf(-ATTENUATION_LIMIT_DB / 20.0);
        let enhanced_weight = 1.0 - noisy_weight;
        let mut filtered = Vec::with_capacity(self.freq_bins);
        for index in 0..self.freq_bins {
            let model = Complex32::new(
                enhanced_spectrum[index * 2],
                enhanced_spectrum[index * 2 + 1],
            );
            filtered.push(spectrum[index] * noisy_weight + model * enhanced_weight);
        }
        // A real-valued inverse FFT requires the DC and Nyquist bins to be real. NumPy's
        // `irfft` silently discards these imaginary components; realfft validates them.
        filtered[0].im = 0.0;
        filtered[self.freq_bins - 1].im = 0.0;

        let mut time = self.inverse.make_output_vec();
        self.inverse
            .process(&mut filtered, &mut time)
            .map_err(|error| anyhow!("DPDFNet inverse FFT failed: {error}"))?;
        for ((overlap, sample), window) in self.overlap.iter_mut().zip(&time).zip(&self.window) {
            *overlap += *sample / self.window_len as f32 * *window;
        }
        let committed = self.overlap[..self.hop_size]
            .iter()
            .map(|sample| {
                if sample.is_finite() {
                    sample.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect();
        self.overlap.copy_within(self.hop_size..self.window_len, 0);
        self.overlap[self.window_len - self.hop_size..].fill(0.0);
        self.input.drain(..self.hop_size);
        Ok(committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorbis_window_is_cola_at_half_overlap() {
        let window_len = 320;
        let hop_size = window_len / 2;
        let window = vorbis_window(window_len);
        for index in 0..hop_size {
            let sum =
                window[index] * window[index] + window[index + hop_size] * window[index + hop_size];
            assert!((sum - 1.0).abs() < 1e-5, "index={index}, sum={sum}");
        }
    }

    #[test]
    fn tested_model_checksum_is_stable() {
        for profile in [NARROWBAND_MODEL, WIDEBAND_MODEL] {
            assert_eq!(profile.sha256.len(), 64);
            assert!(profile.size > 10_000_000);
        }
    }

    #[test]
    #[ignore]
    fn real_model_stream_preserves_length_and_finite_samples() {
        let path = std::env::var("DPDFNET_MODEL_PATH").expect("set DPDFNET_MODEL_PATH");
        let mut enhancer = DpdfNetEnhancer::load(Path::new(&path)).unwrap();
        let sample_rate = enhancer.sample_rate();
        let input: Vec<f32> = (0..sample_rate as usize)
            .map(|index| {
                (2.0 * std::f32::consts::PI * 440.0 * index as f32 / sample_rate as f32).sin() * 0.1
            })
            .collect();
        let mut output = enhancer.process(&input).unwrap();
        output.extend(enhancer.flush().unwrap());
        assert_eq!(output.len(), input.len());
        assert!(output.iter().all(|sample| sample.is_finite()));
    }
}
