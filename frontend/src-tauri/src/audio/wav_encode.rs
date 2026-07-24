// Pure-Rust WAV (PCM) encoder used as a fallback when FFmpeg isn't
// available. Android has no bundled FFmpeg binary (see build.rs and
// audio/ffmpeg.rs's find_ffmpeg_path(), which always returns None there),
// so IncrementalAudioSaver previously had no way to produce a playable
// audio file at all on that platform. These functions write/concatenate
// plain 16-bit PCM mono WAV files using only std::fs/std::io - no external
// process, no native codec dependencies.

use anyhow::{anyhow, Result};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Size of the minimal WAV header this module writes (RIFF + fmt + data,
/// no extra chunks). Every file produced by `write_wav_file` has exactly
/// this many header bytes before its PCM payload starts.
const WAV_HEADER_SIZE: u64 = 44;

/// Write `pcm` (f32 samples in roughly [-1.0, 1.0]) as a 16-bit PCM WAV file.
pub fn write_wav_file(
    pcm: &[f32],
    sample_rate: u32,
    channels: u16,
    output_path: &Path,
) -> Result<()> {
    let bits_per_sample: u16 = 16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;
    let data_size = (pcm.len() * 2) as u32;

    let mut file = std::fs::File::create(output_path)?;
    write_wav_header(&mut file, sample_rate, channels, bits_per_sample, data_size)?;

    // Stream-convert in chunks rather than building a second full-size buffer.
    let mut buf = Vec::with_capacity(64 * 1024);
    for &sample in pcm {
        let v = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
        if buf.len() >= 64 * 1024 {
            file.write_all(&buf)?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        file.write_all(&buf)?;
    }

    Ok(())
}

fn write_wav_header<W: Write>(
    w: &mut W,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    data_size: u32,
) -> Result<()> {
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;
    let file_size = 36 + data_size;

    w.write_all(b"RIFF")?;
    w.write_all(&file_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    w.write_all(&1u16.to_le_bytes())?; // audio format: PCM
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits_per_sample.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_size.to_le_bytes())?;
    Ok(())
}

/// Concatenate the PCM payload of several WAV files (as written by
/// `write_wav_file`, so each has exactly `WAV_HEADER_SIZE` header bytes)
/// into one output WAV file with a single combined header. All inputs must
/// share the same sample rate/channel count/bit depth.
pub fn concat_wav_files(
    inputs: &[PathBuf],
    sample_rate: u32,
    channels: u16,
    output_path: &Path,
) -> Result<()> {
    if inputs.is_empty() {
        return Err(anyhow!("No WAV files to concatenate"));
    }

    let mut total_data_size: u64 = 0;
    for path in inputs {
        let file_len = std::fs::metadata(path)
            .map_err(|e| anyhow!("Checkpoint file missing: {} ({})", path.display(), e))?
            .len();
        if file_len < WAV_HEADER_SIZE {
            return Err(anyhow!(
                "Checkpoint file too small to be a valid WAV: {}",
                path.display()
            ));
        }
        total_data_size += file_len - WAV_HEADER_SIZE;
    }

    let mut out = std::fs::File::create(output_path)?;
    write_wav_header(&mut out, sample_rate, channels, 16, total_data_size as u32)?;

    for path in inputs {
        let mut f = std::fs::File::open(path)?;
        f.seek(SeekFrom::Start(WAV_HEADER_SIZE))?;
        std::io::copy(&mut f, &mut out)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_concat_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        let out = dir.path().join("out.wav");

        write_wav_file(&[0.5, -0.5, 0.25], 48000, 1, &a).unwrap();
        write_wav_file(&[0.1, -0.1], 48000, 1, &b).unwrap();

        concat_wav_files(&[a.clone(), b.clone()], 48000, 1, &out).unwrap();

        let out_len = std::fs::metadata(&out).unwrap().len();
        let a_len = std::fs::metadata(&a).unwrap().len();
        let b_len = std::fs::metadata(&b).unwrap().len();
        assert_eq!(out_len, a_len + b_len - WAV_HEADER_SIZE);
    }
}
