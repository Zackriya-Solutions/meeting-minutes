//! Read-only detector smoke test, independent of the full transcription build.
//! rustc --edition=2021 scripts/call-detection-probe.rs -o /tmp/meetily-call-probe
//! /tmp/meetily-call-probe [--browsers]
#[cfg(target_os = "macos")]
#[path = "../frontend/src-tauri/src/meeting_detection/macos.rs"]
mod macos;
#[path = "../frontend/src-tauri/src/meeting_detection/signals.rs"]
mod signals;

fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("supported={}", macos::supported());
        match macos::probe(false, std::env::args().any(|arg| arg == "--browsers")) {
            Ok(label) => println!("detected={}", label.as_deref().unwrap_or("none")),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    println!("supported=false");
}
