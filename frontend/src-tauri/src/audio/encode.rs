use super::ffmpeg::find_ffmpeg_path; // Correct path to encode module
use super::AudioDevice;
use std::io::{Read, Write};
use std::sync::Arc;
use std::{
    path::PathBuf,
    process::{Child, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tracing::{debug, error};

/// FFmpeg normally encodes a 30-second checkpoint in well under a second. Keep a generous
/// ceiling for slow machines while ensuring a wedged process cannot hold recording shutdown
/// forever.
pub(crate) const FFMPEG_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub struct AudioInput {
    pub data: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub device: Arc<AudioDevice>,
}

pub fn encode_single_audio(
    data: &[u8],
    sample_rate: u32,
    channels: u16,
    output_path: &PathBuf,
) -> anyhow::Result<()> {
    debug!(
        "Starting FFmpeg process for {} bytes of audio data",
        data.len()
    );

    if data.is_empty() {
        return Err(anyhow::anyhow!("No audio data provided for encoding"));
    }

    let ffmpeg_path = find_ffmpeg_path().ok_or_else(|| {
        anyhow::anyhow!("FFmpeg not found. Please install FFmpeg to save recordings.")
    })?;

    debug!("Using FFmpeg at: {:?}", ffmpeg_path);

    let mut command = Command::new(ffmpeg_path);
    command
        .args([
            "-f",
            "f32le",
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &channels.to_string(),
            "-i",
            "pipe:0",
            "-c:a",
            "aac",
            "-b:a",
            "192k", // Increased from 64k for better audio quality (especially for speech)
            "-profile:a",
            "aac_low", // Use AAC-LC profile for better compatibility
            "-movflags",
            "+faststart", // Optimize for web streaming
            "-f",
            "mp4",
        ])
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Hide console window on Windows to prevent CMD popup during recording
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    debug!("FFmpeg command: {:?}", command);

    #[allow(clippy::zombie_processes)] // ownership moves into a helper that always waits/reaps
    let ffmpeg = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn FFmpeg process: {e}"))?;
    debug!("FFmpeg process spawned");
    debug!("Writing audio and waiting for FFmpeg process to exit");
    let output = wait_for_process_output_with_timeout(
        ffmpeg,
        Some(data),
        FFMPEG_PROCESS_TIMEOUT,
        "FFmpeg encode",
    )?;
    let status = output.status;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    debug!("FFmpeg process exited with status: {}", status);
    debug!("FFmpeg stdout: {}", stdout);
    debug!("FFmpeg stderr: {}", stderr);

    if !status.success() {
        error!("FFmpeg process failed with status: {}", status);
        error!("FFmpeg stderr: {}", stderr);
        return Err(anyhow::anyhow!(
            "FFmpeg process failed with status: {}",
            status
        ));
    }

    Ok(())
}

/// Wait for a child while continuously draining its output. Input is written on a companion
/// thread so the same deadline also covers a child that stops reading stdin. On timeout the
/// child is killed and reaped before this function returns.
pub(crate) fn wait_for_process_output_with_timeout(
    mut child: Child,
    input: Option<&[u8]>,
    timeout: Duration,
    process_name: &str,
) -> anyhow::Result<Output> {
    let child_stdin = if input.is_some() {
        match child.stdin.take() {
            Some(stdin) => Some(stdin),
            None => {
                terminate_child(&mut child);
                return Err(anyhow::anyhow!("Failed to open {process_name} stdin"));
            }
        }
    } else {
        // Explicitly close any configured stdin so a child cannot wait for input forever.
        drop(child.stdin.take());
        None
    };

    let stdout_reader = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        })
    });
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        })
    });

    let (status_result, write_result) = thread::scope(|scope| {
        let writer = child_stdin.map(|mut stdin| {
            let input = input.expect("stdin is only taken when input is present");
            scope.spawn(move || stdin.write_all(input))
        });

        let status_result = wait_for_exit_with_timeout(&mut child, timeout, process_name);
        let write_result = writer.map(|writer| {
            writer
                .join()
                .map_err(|_| anyhow::anyhow!("{process_name} stdin writer panicked"))?
                .map_err(|e| anyhow::anyhow!("Failed to write {process_name} stdin: {e}"))
        });

        (status_result, write_result)
    });

    let stdout = join_output_reader(stdout_reader, process_name, "stdout")?;
    let stderr = join_output_reader(stderr_reader, process_name, "stderr")?;
    let status = status_result?;

    // A failed child commonly closes stdin early. Preserve its exit status and stderr instead
    // of replacing the useful diagnostic with a BrokenPipe error.
    if status.success() {
        if let Some(write_result) = write_result {
            write_result?;
        }
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn wait_for_exit_with_timeout(
    child: &mut Child,
    timeout: Duration,
    process_name: &str,
) -> anyhow::Result<ExitStatus> {
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= timeout => {
                terminate_child(child);
                return Err(anyhow::anyhow!(
                    "{process_name} timed out after {} seconds and was terminated",
                    timeout.as_secs_f64()
                ));
            }
            Ok(None) => {
                let remaining = timeout.saturating_sub(started.elapsed());
                thread::sleep(PROCESS_POLL_INTERVAL.min(remaining));
            }
            Err(e) => {
                terminate_child(child);
                return Err(anyhow::anyhow!(
                    "Failed while waiting for {process_name}: {e}"
                ));
            }
        }
    }
}

fn terminate_child(child: &mut Child) {
    // kill() may report that the process already exited between try_wait and this call. wait()
    // is still required to reap it, so both operations are intentionally best-effort.
    let _ = child.kill();
    let _ = child.wait();
}

fn join_output_reader(
    reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    process_name: &str,
    stream_name: &str,
) -> anyhow::Result<Vec<u8>> {
    match reader {
        Some(reader) => reader
            .join()
            .map_err(|_| anyhow::anyhow!("{process_name} {stream_name} reader panicked"))?
            .map_err(|e| anyhow::anyhow!("Failed to read {process_name} {stream_name}: {e}")),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_timeout_kills_and_reaps_a_hung_child() {
        #[cfg(unix)]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "exec sleep 10"]);
            command
        };

        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping 127.0.0.1 -n 10 >NUL"]);
            command
        };

        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().unwrap();
        let started = Instant::now();

        let error = wait_for_process_output_with_timeout(
            child,
            None,
            Duration::from_millis(50),
            "test process",
        )
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
