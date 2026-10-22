// audio/transcription/gemini_live/pcm.rs
//
// f32 samples -> 16-bit little-endian PCM frames, the wire format Gemini Live expects
// (`audio/pcm;rate=16000`).

/// 100 ms of audio at 16 kHz, the chunk size recommended by the Live API docs.
pub const FRAME_SAMPLES: usize = 1600;
pub const SAMPLE_RATE: u32 = 16000;

/// Convert f32 samples in [-1.0, 1.0] to s16le bytes, clamping out-of-range values.
pub fn f32_to_s16le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Split samples into 100 ms s16le frames (the last frame may be shorter).
pub fn frames(samples: &[f32]) -> impl Iterator<Item = Vec<u8>> + '_ {
    samples.chunks(FRAME_SAMPLES).map(f32_to_s16le)
}

pub fn duration_secs(samples: &[f32]) -> f64 {
    samples.len() as f64 / SAMPLE_RATE as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_clamps() {
        let bytes = f32_to_s16le(&[0.0, 1.0, -1.0, 2.0, -2.0, 0.5]);
        let vals: Vec<i16> = bytes
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(vals, vec![0, 32767, -32767, 32767, -32767, 16384]);
    }

    #[test]
    fn frames_are_100ms() {
        let samples = vec![0.1f32; FRAME_SAMPLES * 2 + 10];
        let f: Vec<_> = frames(&samples).collect();
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].len(), FRAME_SAMPLES * 2);
        assert_eq!(f[2].len(), 20);
    }
}
