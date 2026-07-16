use anyhow::{anyhow, Context, Result};
use app_lib::qwen_asr_engine::{ModelStatus, QwenAsrEngine, QWEN3_ASR_MODEL};
use sherpa_onnx::Wave;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: qwen_asr_smoke <models-dir> <16khz-mono-wav>"))?,
    );
    let wav_path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: qwen_asr_smoke <models-dir> <16khz-mono-wav>"))?,
    );

    let engine = QwenAsrEngine::new(models_dir)?;
    if engine.discover_model().status != ModelStatus::Available {
        engine
            .download_model(|progress| {
                eprint!(
                    "\rDownloading Qwen3-ASR: {:3}% ({:.1}/{:.1} MiB, {:.1} MiB/s)",
                    progress.percent,
                    progress.downloaded_mb,
                    progress.total_mb,
                    progress.speed_mbps
                );
            })
            .await?;
        eprintln!();
    }

    engine.load_model(QWEN3_ASR_MODEL).await?;
    let wav_path_str = wav_path
        .to_str()
        .ok_or_else(|| anyhow!("Smoke-test WAV path is not valid UTF-8"))?;
    let wave = Wave::read(wav_path_str)
        .with_context(|| format!("Failed to read {}", wav_path.display()))?;
    if wave.sample_rate() != 16_000 {
        return Err(anyhow!("Smoke-test WAV must be 16 kHz mono"));
    }
    let text = engine.transcribe_audio(wave.samples().to_vec()).await?;
    println!("{text}");
    if text.trim().is_empty() {
        return Err(anyhow!("Qwen3-ASR returned an empty transcript"));
    }
    Ok(())
}
