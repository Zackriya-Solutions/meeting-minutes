use anyhow::{anyhow, Context, Result};
use app_lib::sense_voice_engine::{ModelStatus, SenseVoiceEngine, SENSE_VOICE_MODEL};
use sherpa_onnx::Wave;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: sense_voice_smoke <models-dir> <16khz-mono-wav>"))?,
    );
    let wav_paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if wav_paths.is_empty() {
        return Err(anyhow!(
            "usage: sense_voice_smoke <models-dir> <16khz-mono-wav> [...]"
        ));
    }

    let engine = SenseVoiceEngine::new(models_dir)?;
    if engine.discover_model().status != ModelStatus::Available {
        engine
            .download_model(|progress| {
                eprint!(
                    "\rDownloading SenseVoice: {:3}% ({:.1}/{:.1} MiB, {:.1} MiB/s)",
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
    engine.load_model(SENSE_VOICE_MODEL).await?;
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
        let text = engine.transcribe_audio(wave.samples().to_vec()).await?;
        println!(
            "{}\t{:.2}s\t{}",
            wav_path.display(),
            started.elapsed().as_secs_f64(),
            text
        );
        if text.trim().is_empty() {
            return Err(anyhow!("SenseVoice returned an empty transcript"));
        }
    }
    Ok(())
}
