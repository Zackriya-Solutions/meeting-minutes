# Qwen3-ASR 1.7B ONNX Validation

Validation date: 2026-07-17

## Artifact

- Model: `qwen3-asr-1.7b-int8`
- Repository: `ilmina/qwen3-asr-1.7b-sherpa-onnx`
- Revision: `66fb5ea2d4d1682ff8a663bf7e788913604996a0`
- Download size: 2,292.8 MiB
- Verification: all six files matched their pinned sizes and SHA-256 hashes

The community repository has no model card or separate license metadata. Technical validation does not replace a provenance and license review before distribution.

## Environment

- Hardware: Apple M2 Max, 12 CPU cores, 96 GB memory
- Runtime: sherpa-onnx 1.13.4, CPU provider, three inference threads
- Audio: 16 kHz mono PCM generated with the macOS Tingting and Samantha system voices
- Language mode: automatic detection

## Results

| Sample | Audio duration | Inference | RTF | Output |
| --- | ---: | ---: | ---: | --- |
| Mandarin Chinese | 6.86 s | 3.53 s | 0.51 | Exact sentence match |
| English | 6.68 s | 3.80 s | 0.57 | Sentence match; punctuation normalized |

Model loading took 2.73 seconds. A single process loading the model and transcribing both samples used 4,086,071,296 bytes (3.81 GiB) maximum resident memory. macOS `time -l` reported 23.61 user CPU seconds and 0.85 system CPU seconds over 10.15 seconds wall time, approximately 241% aggregate CPU for load plus both transcriptions.

For comparison, the 0.6B model loaded in 1.71 seconds and transcribed the same Mandarin sample in 1.51 seconds (RTF 0.22).

## Command

```bash
LIBONNXRUNTIME_NO_PKG_CONFIG=1 \
QWEN_ASR_MODEL=qwen3-asr-1.7b-int8 \
QWEN_ASR_LANGUAGE=auto \
cargo run --example qwen_asr_smoke -- \
  "$HOME/Library/Application Support/com.meetily.ai/models" \
  /tmp/meetily-qwen-zh.wav \
  /tmp/meetily-qwen-en.wav
```
