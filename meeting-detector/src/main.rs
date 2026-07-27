//! Memento auto-recorder: a tiny tray companion for the Memento/Meetily desktop app.
//!
//! It lives in the macOS menu bar, watches for a recognized meeting client taking
//! over the microphone (the same heuristic the main app uses), records mic-only
//! audio while the call is live, and — when the call ends — drops the recording
//! into the main app's recordings folder and registers it in the meeting list so
//! it can be transcribed later with the "Enhance" button.
//!
//! Intentionally lean: no webview, no whisper, no async runtime.

mod paths;

#[cfg(target_os = "macos")]
mod app;

#[cfg(target_os = "macos")]
mod detection;

#[cfg(target_os = "macos")]
mod recorder;

#[cfg(target_os = "macos")]
mod system_audio;

#[cfg(target_os = "macos")]
mod mixer;

#[cfg(target_os = "macos")]
mod register;

#[cfg(target_os = "macos")]
mod login_item;

fn main() {
    // Default to `info`; override with RUST_LOG=debug for verbose detection logs.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(target_os = "macos")]
    {
        // `--selftest [seconds]` records mic + system audio to a WAV you can play
        // back, to confirm capture works (and surface the permission prompts) once,
        // without needing a live meeting.
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--selftest") {
            let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);
            if let Err(e) = selftest(seconds) {
                eprintln!("selftest failed: {e:#}");
                std::process::exit(1);
            }
            return;
        }

        app::run();
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!(
            "memento-detector currently supports macOS only \
             (microphone-session detection relies on CoreAudio)."
        );
        std::process::exit(1);
    }
}

/// Record a few seconds of mixed mic + system audio to a WAV for manual validation.
#[cfg(target_os = "macos")]
fn selftest(seconds: u64) -> anyhow::Result<()> {
    use std::time::Duration;

    let out = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("memento-detector-selftest.wav");

    println!("Recording {seconds}s of mic + system audio -> {}", out.display());
    println!("Tip: play a video AND talk into your mic so you can confirm both are captured.");

    let recorder = recorder::Recorder::start(out.clone())?;
    std::thread::sleep(Duration::from_secs(seconds));
    let info = recorder.stop()?;

    println!(
        "Done: {} ({:.1}s). Play it back — you should hear BOTH your voice and system audio.",
        info.wav_path.display(),
        info.duration_secs
    );
    Ok(())
}
