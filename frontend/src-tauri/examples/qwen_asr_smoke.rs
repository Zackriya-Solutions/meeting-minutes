use anyhow::{anyhow, Context, Result};
use app_lib::qwen_asr_engine::{ModelStatus, QwenAsrEngine, QWEN3_ASR_MODEL};
use sherpa_onnx::Wave;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: qwen_asr_smoke <models-dir> <16khz-mono-wav>"))?,
    );
    let wav_paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if wav_paths.is_empty() {
        return Err(anyhow!(
            "usage: qwen_asr_smoke <models-dir> <16khz-mono-wav> [...]"
        ));
    }
    let language = std::env::var("QWEN_ASR_LANGUAGE").ok();
    let model_name =
        std::env::var("QWEN_ASR_MODEL").unwrap_or_else(|_| QWEN3_ASR_MODEL.to_string());

    let engine = QwenAsrEngine::new(models_dir)?;
    let model = engine
        .discover_models()
        .into_iter()
        .find(|model| model.name == model_name)
        .ok_or_else(|| anyhow!("Unknown Qwen3-ASR model: {model_name}"))?;
    if model.status != ModelStatus::Available {
        engine
            .download_model(&model_name, |progress| {
                eprint!(
                    "\rDownloading {model_name}: {:3}% ({:.1}/{:.1} MiB, {:.1} MiB/s)",
                    progress.percent,
                    progress.downloaded_mb,
                    progress.total_mb,
                    progress.speed_mbps
                );
            })
            .await?;
        eprintln!();
    }

    let load_started = Instant::now();
    engine.load_model(&model_name).await?;
    eprintln!(
        "Model loaded in {:.2}s",
        load_started.elapsed().as_secs_f64()
    );
    for wav_path in wav_paths {
        let wav_path_str = wav_path
            .to_str()
            .ok_or_else(|| anyhow!("Smoke-test WAV path is not valid UTF-8"))?;
        let wave = Wave::read(wav_path_str)
            .with_context(|| format!("Failed to read {}", wav_path.display()))?;
        if wave.sample_rate() != 16_000 {
            return Err(anyhow!("Smoke-test WAV must be 16 kHz mono"));
        }
        let started = Instant::now();
        let text = engine
            .transcribe_audio(wave.samples().to_vec(), language.clone())
            .await?;
        println!(
            "{}\t{:.2}s\t{}",
            wav_path.display(),
            started.elapsed().as_secs_f64(),
            text
        );
        if text.trim().is_empty() {
            return Err(anyhow!("Qwen3-ASR returned an empty transcript"));
        }
    }
    Ok(())
}
