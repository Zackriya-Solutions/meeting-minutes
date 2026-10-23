// Stress harness for the Linux PulseAudio enumeration lifecycle (TECH-02).
//
// Reproduces the exact concurrency shape observed during the 2026-08-13
// SIGABRT (`malloc(): unaligned tcache chunk detected`):
//
//   - two long-lived `pa_simple` capture streams (microphone + system audio)
//     each running on its own std::thread;
//   - one enumerator thread that repeatedly calls `list_sources()` then
//     `list_sinks()`, compressing the production poll interval (2 s) so a
//     one-hour meeting is exercised in ~6 minutes.
//
// The harness is intentionally free of Tauri, Whisper, the UI and the audio
// pipeline. It can therefore run under heavy instrumentation (valgrind,
// MALLOC_CHECK_=3) at a tolerable cost.
//
// Usage:
//
//   cd frontend/src-tauri
//
//   # 1. Nominal (release, fast)
//   cargo run --release --example pulse_stress
//
//   # 2. glibc heap paranoia — abort as close to the fault as possible
//   MALLOC_CHECK_=3 MALLOC_PERTURB_=204 cargo run --release --example pulse_stress
//
//   # 3. Valgrind — locates the exact faulty write. Slow (~20x) but the
//   #    harness is light. Leaks and "still reachable" blocks from libpulse /
//   #    PipeWire are expected and ignored; look for Invalid read/write/free
//   #    attributed to pulse_linux / pulse_enumerator code.
//   PULSE_STRESS_SECS=180 PULSE_STRESS_ENUM_MS=500 \
//     cargo build --release --example pulse_stress && \
//     valgrind --leak-check=no --track-origins=yes --error-limit=no \
//       ./target/release/examples/pulse_stress
//
// Configuration (environment variables):
//   PULSE_STRESS_SECS    total duration in seconds (default: 300)
//   PULSE_STRESS_ENUM_MS enumerator interval in milliseconds (default: 200)

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use app_lib::audio::capture::pulse_linux::{
    list_sinks, list_sources, PulseCapture,
};

fn main() {
    env_logger::init();

    let total_secs = std::env::var("PULSE_STRESS_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300_u64);
    let enum_ms = std::env::var("PULSE_STRESS_ENUM_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_u64);
    let enum_interval = Duration::from_millis(enum_ms);
    let run_duration = Duration::from_secs(total_secs);

    println!(
        "TECH-02 pulse_stress: duration={}s enum_interval={}ms",
        total_secs, enum_ms
    );

    // Resolve real devices before starting the long-lived threads so that an
    // empty device list fails fast and clearly.
    let sources = list_sources().expect("Failed to list PulseAudio sources");
    let source = sources
        .into_iter()
        .next()
        .expect("No real input source available for microphone thread");
    println!("Microphone source: {} ({})", source.description, source.source_name);

    let sinks = list_sinks().expect("Failed to list PulseAudio sinks");
    let sink = sinks
        .into_iter()
        .next()
        .expect("No sink available for system-audio thread");
    println!("System sink: {} ({})", sink.description, sink.monitor_source_name);

    let start = Instant::now();
    let deadline = start + run_duration;

    let enum_errors = Arc::new(AtomicUsize::new(0));
    let enum_cycles = Arc::new(AtomicUsize::new(0));

    // ---- Microphone capture thread ----
    let mic_capture =
        PulseCapture::new_microphone(&source.source_name).expect("Failed to open microphone capture");
    let mic_stop = mic_capture.stop_handle();
    let mic_samples = Arc::new(AtomicUsize::new(0));
    let mic_samples_worker = mic_samples.clone();
    let mic_handle = thread::spawn(move || {
        mic_capture.run(|samples| {
            mic_samples_worker.fetch_add(samples.len(), Ordering::Relaxed);
        });
    });

    // ---- System-audio capture thread ----
    let sys_capture = PulseCapture::new_system(&sink.monitor_source_name)
        .expect("Failed to open system-audio capture");
    let sys_stop = sys_capture.stop_handle();
    let sys_samples = Arc::new(AtomicUsize::new(0));
    let sys_samples_worker = sys_samples.clone();
    let sys_handle = thread::spawn(move || {
        sys_capture.run(|samples| {
            sys_samples_worker.fetch_add(samples.len(), Ordering::Relaxed);
        });
    });

    // ---- Enumeration stress thread ----
    let enum_errors_worker = enum_errors.clone();
    let enum_cycles_worker = enum_cycles.clone();
    let enum_handle = thread::spawn(move || {
        while Instant::now() < deadline {
            if let Err(e) = list_sources() {
                eprintln!("list_sources error: {}", e);
                enum_errors_worker.fetch_add(1, Ordering::Relaxed);
            }
            if let Err(e) = list_sinks() {
                eprintln!("list_sinks error: {}", e);
                enum_errors_worker.fetch_add(1, Ordering::Relaxed);
            }
            enum_cycles_worker.fetch_add(1, Ordering::Relaxed);
            thread::sleep(enum_interval);
        }
    });

    // Wait for the stress duration.
    enum_handle.join().expect("Enumeration thread panicked");

    // Signal captures to stop and wait for clean shutdown.
    mic_stop.store(true, Ordering::Release);
    sys_stop.store(true, Ordering::Release);
    mic_handle.join().expect("Microphone thread panicked");
    sys_handle.join().expect("System-audio thread panicked");

    let elapsed = start.elapsed();
    let cycles = enum_cycles.load(Ordering::Relaxed);
    let errors = enum_errors.load(Ordering::Relaxed);
    let mic_total = mic_samples.load(Ordering::Relaxed);
    let sys_total = sys_samples.load(Ordering::Relaxed);

    println!("--- TECH-02 pulse_stress report ---");
    println!("Elapsed:            {:?}", elapsed);
    println!("Enumeration cycles: {}", cycles);
    println!("Enumeration errors: {}", errors);
    println!("Microphone samples: {}", mic_total);
    println!("System samples:     {}", sys_total);

    if errors > 0 || mic_total == 0 || sys_total == 0 {
        eprintln!("FAILURE: non-zero enumeration errors or zero sample counts");
        std::process::exit(1);
    }

    println!("SUCCESS");
}
