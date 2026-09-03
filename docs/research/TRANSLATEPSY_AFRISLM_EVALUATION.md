# TranslatePsy-AfriSLM Evaluation (vs Parakeet, for African-language support)

**Date**: 2026-09-03
**Question**: Is TranslatePsy-AfriSLM a good drop-in replacement or addition to Parakeet for African languages? What is its accuracy vs Parakeet, and should we consider it?
**Verdict**: Not a Parakeet replacement, because it is not a speech model at all. Worth considering as an additive local translation layer.

## What TranslatePsy-AfriSLM actually is

A real, downloadable release (2026-09-02) from Tether's QVAC AI division. It is a **text-to-text machine translation model family**, not ASR:

- Decoder-only LLM fine-tuned from Qwen3.5, sizes 0.8B / 2B / 4B.
- Covers 19-20 Sub-Saharan African languages: Afrikaans, Amharic, Hausa, Igbo, Kinyarwanda, Lingala, Luganda, Malagasy, Nyanja, Oromo, Nigerian Pidgin, Shona, Somali, Southern Sotho, Swahili, Tswana, Wolof, Xhosa, Yoruba, Zulu.
- Weights on Hugging Face in safetensors and GGUF Q8/Q4: https://huggingface.co/collections/qvac/translatepsy-afrislm
- License: vendor-stated Apache 2.0 (per-model-card tags not independently verified). The accompanying synthetic training/eval dataset is CC BY-NC 4.0.
- Paper: "TranslatePsy-AfriSLM: High-Quality Data Scaling For Low-Resource Machine Translation", arXiv:2608.18655 (Tether AI Research).

## Accuracy vs Parakeet

No valid comparison exists; framing them as competitors conflates model types:

- **Parakeet** (TDT-0.6B-v3) is ASR for 25 **European** languages (~6.3% avg WER, 11.97% FLEURS). It has zero African-language support, so for African languages our current Parakeet path offers nothing to compare against.
- **AfriSLM benchmarks are translation metrics** (SSA-COMET, chrF++, spBLEU on FLORES-200 / BOUQuET / Smol), not WER. Self-reported results show even the 0.8B beating Qwen3.5-122B-A10B, TranslateGemma-27B, and NLLB-3.3B on SSA-COMET (e.g. BOUQuET SSA-COMET: 0.8B 0.6223, 2B 0.6322, 4B 0.6391 vs Qwen3.5-122B 0.5716, NLLB-3.3B 0.6178). Not independently reproduced.
- **Whisper large-v3** covers ~99 languages but is weak/underspecified on most Sub-Saharan ones. Community fine-tunes exist (e.g. SALT "Whisper 51 African Languages", ~10-15% WER on well-resourced languages like Swahili): https://salt.sunbird.ai/models/asr-whisper-51-african-languages/

## Fit for PulseTalk

**As a Parakeet drop-in: no.**

1. It takes text in, not audio, so it cannot implement our `TranscriptionProvider` trait (`frontend/src-tauri/src/audio/transcription/provider.rs`), which is audio f32 in, transcript out.
2. Even for a genuine ASR swap, the Parakeet path is tightly coupled to the NeMo TDT ONNX export: hardcoded tensor names and LSTM state shapes, a TDT duration-logit decoder, a `nemo128.onnx` preprocessor graph, and a hand-rolled `vocab.txt` tokenizer in `frontend/src-tauri/src/parakeet_engine/model.rs`. Model registry, file names, byte-size validation, and download URLs are all Rust literals in `parakeet_engine.rs`, not config-driven. Only the outer `TranscriptionProvider` trait is genuinely pluggable. The Parakeet provider also currently ignores the `language` parameter entirely.

**As an addition: plausibly yes.** The realistic architecture for African-language meetings is a two-stage pipeline:

1. An African-capable ASR model for transcription (a Whisper fine-tune such as SALT's, since stock Whisper is weak there and Parakeet is unusable). This is the blocking gap and a separate model search.
2. AfriSLM as a local translation layer for transcripts and summaries. The GGUF quants (0.8B Q4 is a few hundred MB) fit the local-first story and could run through the existing Ollama integration with near-zero Rust work.

## Recommendation

Track as a "local African-language translation feature", not a "Parakeet alternative". Before committing:

- Verify the Apache 2.0 license on the actual HF model cards (only the vendor's aggregate claim was found).
- Spot-check translation quality on a couple of target languages; all benchmarks are self-published by Tether AI Research.

## Caveats

- The originating Reddit post (u/QVAC_Official) could not be fetched (Reddit blocks automated fetching), so its exact claims are unconfirmed. All findings above come from the arXiv paper, QVAC's site (https://qvac.tether.io/models/), and Hugging Face.
- Corporate provenance is unusual (Tether, the USDT stablecoin issuer); not a technical red flag, but worth noting.
