# Transcription Model Benchmark Analysis

## Scope

This document benchmarks transcription models currently used by this repository for recorded meeting transcription, with special focus on Spanish.

## Models currently used in this repo

Based on repository defaults and runtime selection logic:

- **Default recording transcription provider:** `parakeet`
- **Default Parakeet model:** `parakeet-tdt-0.6b-v3-int8`
- **Default Whisper model when `localWhisper` is selected:** `large-v3-turbo`

Evidence in repo:

- `frontend/src-tauri/src/database/commands.rs` initializes fresh DB with provider `parakeet`
- `frontend/src-tauri/src/config.rs` sets:
  - `DEFAULT_PARAKEET_MODEL = "parakeet-tdt-0.6b-v3-int8"`
  - `DEFAULT_WHISPER_MODEL = "large-v3-turbo"`
- `frontend/src-tauri/src/audio/transcription/engine.rs` falls back to `parakeet` when no transcript config exists
- `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` also exposes optional `parakeet-ctc-es-0.6b-int8` as a Spanish-first beta model, but it is **not** default

## Sources used

### Hugging Face / leaderboard / model cards

1. Hugging Face model card: `openai/whisper-large-v3-turbo`
   - https://huggingface.co/openai/whisper-large-v3-turbo
2. Hugging Face model card: `openai/whisper-large-v3`
   - https://huggingface.co/openai/whisper-large-v3
3. Hugging Face model card: `nvidia/parakeet-tdt-0.6b-v3`
   - https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
4. Hugging Face model card: `sonic-speech/parakeet-tdt-0.6b-v3-int8`
   - https://huggingface.co/sonic-speech/parakeet-tdt-0.6b-v3-int8
5. Hugging Face Open ASR Leaderboard repo + CSV data
   - https://github.com/huggingface/open_asr_leaderboard
   - https://raw.githubusercontent.com/huggingface/open_asr_leaderboard/main/scripts/data/multilingual.csv
   - https://raw.githubusercontent.com/huggingface/open_asr_leaderboard/main/scripts/data/en_shortform.csv

### Additional independent sources

6. TheStageAI multilingual benchmark summary
   - https://github.com/TheStageAI/TheWhisper/blob/main/benchmark/README.md
7. Novascribe comparative analysis for Whisper large-v3 vs turbo
   - https://novascribe.ai/whisper-large-v3-vs-turbo
8. OpenAI discussion for Whisper turbo tradeoff context
   - https://github.com/openai/whisper/discussions/2363

## Important methodology note

This comparison mixes three evidence types:

1. **HF leaderboard data**: strongest source for cross-model comparison because hardware and evaluation flow are normalized
2. **HF model cards**: strongest source for official model scope, sizes, language coverage, and vendor-reported claims
3. **Independent blogs / benchmark repos**: useful for interpretation, but lower confidence than normalized leaderboard CSVs

When numbers disagree, prefer **HF leaderboard CSV** first.

## Model scope and official positioning

### Whisper large-v3

From Hugging Face model card:

- Multilingual ASR + translation model
- Size: **1550M parameters**
- Card claim: **10% to 20% error reduction vs Whisper large-v2** across many languages
- Spanish-specific numbers are **not** listed in the official card

### Whisper large-v3-turbo

From Hugging Face model card:

- Multilingual ASR model
- Size: **809M parameters**
- It is a **pruned / distilled** large-v3 variant
- Decoder layers reduced from **32 to 4**
- Official claim: **"way faster, at the expense of a minor quality degradation"**
- Spanish-specific numbers are **not** listed in the official card

### NVIDIA Parakeet TDT 0.6B v3

From Hugging Face model card:

- **600M multilingual ASR** model
- Automatic language detection
- Supports **25 European languages**, including **Spanish**
- Spanish is explicitly benchmarked in the model card

## Hugging Face Open ASR Leaderboard — multilingual results relevant to Spanish

Source:
- `multilingual.csv` from HF Open ASR Leaderboard

Extracted rows:

| Model | RTFx | es_covost | es_mls | es_fleurs | Overall multilingual Avg WER |
|---|---:|---:|---:|---:|---:|
| `openai/whisper-large-v3` | 110.92 | 4.47 | 4.16 | 2.31 | 4.81 |
| `openai/whisper-large-v3-turbo` | 176.16 | 6.35 | 3.96 | 2.70 | 5.56 |
| `nvidia/parakeet-tdt-0.6b-v3` | 1719.32 | 3.55 | 4.41 | 3.24 | 4.81 |

### Spanish-only mean across available HF multilingual columns

Simple average of `es_covost`, `es_mls`, `es_fleurs`:

| Model | Spanish mean WER |
|---|---:|
| `openai/whisper-large-v3` | **3.65** |
| `nvidia/parakeet-tdt-0.6b-v3` | **3.73** |
| `openai/whisper-large-v3-turbo` | **4.34** |

### Reading these numbers

- **Best average Spanish accuracy in HF multilingual CSV:** `whisper-large-v3`
- **Very close second:** `parakeet-tdt-0.6b-v3`
- **Fast but weaker on Spanish average:** `whisper-large-v3-turbo`
- Biggest turbo weakness here is **Spanish CoVoST**: `6.35`, clearly worse than `large-v3` `4.47` and Parakeet `3.55`

## Hugging Face Open ASR Leaderboard — English short-form reference

Source:
- `en_shortform.csv` from HF Open ASR Leaderboard

| Model | Avg WER | RTFx | AMI | Earnings22 | GigaSpeech | LS Clean | LS Other | SPGI | Tedlium | Voxpopuli |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `openai/whisper-large-v3` | 7.44 | 145.51 | 15.95 | 11.29 | 10.02 | 2.01 | 3.91 | 2.94 | 3.86 | 9.54 |
| `openai/whisper-large-v3-turbo` | 7.83 | 200.19 | 16.13 | 11.63 | 10.14 | 2.10 | 4.24 | 2.97 | 3.57 | 11.87 |
| `nvidia/parakeet-tdt-0.6b-v3` | **6.32** | **3332.74** | 11.39 | 11.19 | 9.57 | 1.92 | 3.59 | 3.98 | 2.80 | 6.09 |

### Reading these numbers

- On English short-form leaderboard data, Parakeet is both **more accurate** and **dramatically faster**
- `parakeet-tdt-0.6b-v3` RTFx `3332.74` vs:
  - `whisper-large-v3` `145.51`
  - `whisper-large-v3-turbo` `200.19`
- That is roughly:
  - **22.9x faster than Whisper large-v3**
  - **16.6x faster than Whisper large-v3-turbo**

## Spanish-specific numbers from NVIDIA Parakeet model card

Source:
- https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3

Model card reports Spanish explicitly:

| Dataset | Spanish WER |
|---|---:|
| FLEURS (`es`) | 3.45 |
| MLS (`es`) | 4.39 |
| CoVoST (`es`) | 3.41 |

Model card multilingual averages:

- FLEURS average: **11.97%**
- MLS average: **7.83%**
- CoVoST average: **11.98%**

These numbers are directionally consistent with HF leaderboard Spanish rows: Parakeet is strong in Spanish, not merely English.

## INT8 variant note for repo default model

Repo default is not generic Parakeet BF16. It is:

- `parakeet-tdt-0.6b-v3-int8`

Relevant source:
- https://huggingface.co/sonic-speech/parakeet-tdt-0.6b-v3-int8

Vendor-reported benchmark from that model card:

| Metric | BF16 | INT8 | Change |
|---|---:|---:|---:|
| WER (LibriSpeech) | 0.82% | 0.82% | none |
| WER (TED-LIUM) | 15.1% | 15.1% | none |
| RTFx | 73x | 95x | +30% |
| Peak memory | 3002 MB | 1268 MB | -58% |
| Weight size | 1254 MB | 755 MB | -40% |

Important caveat:

- This INT8 benchmark is **vendor/model-card reported**, not the normalized HF Open ASR leaderboard
- It is also reported on **Apple M3 Max + MLX**, so it should be used as **directional evidence**, not as directly comparable to the leaderboard RTFx values
- Still, it aligns with repo choice: **INT8 variant exists to keep quality roughly intact while reducing memory and improving throughput**

## Additional external benchmark signals

### TheStageAI multilingual benchmark summary

Source:
- https://github.com/TheStageAI/TheWhisper/blob/main/benchmark/README.md

Reported Spanish WER table:

| Model | Spanish WER |
|---|---:|
| `TheWhisper` | 3.14 |
| `openai/whisper-large-v3-turbo` | 3.94 |
| `nvidia/parakeet-tdt-0.6b-v3` | 3.75 |
| `nvidia/canary-1b-v2` | 3.22 |

Also reported overall multilingual leaderboard average:

| Model | Avg multilingual WER |
|---|---:|
| `TheWhisper` | 4.30 |
| `microsoft/Phi-4-multimodal-instruct` | 4.60 |
| `nvidia/canary-1b-v2` | 4.89 |
| `nvidia/parakeet-tdt-0.6b-v3` | 5.05 |
| `openai/whisper-large-v3-turbo` | 5.44 |

This is not official HF leaderboard output, but it reinforces same pattern: **turbo is usually weaker than top multilingual contenders, including Parakeet, when judged by WER**.

### Novascribe analysis for large-v3 vs turbo

Source:
- https://novascribe.ai/whisper-large-v3-vs-turbo

Reported takeaways:

- Open ASR composite aggregate WER:
  - `whisper-large-v3`: **~7.44%**
  - `whisper-large-v3-turbo`: **~7.70%**
- For Tier-1 European languages including **Spanish**, their summary says turbo is usually within roughly **+0.5 to +2 pp** vs large-v3, but not best-in-class

Use this as interpretive context only, not primary evidence.

## Practical conclusions for this repo

### 1. Current default choice (`parakeet-tdt-0.6b-v3-int8`) makes sense

Why:

- Default app path values **speed and local usability**
- Parakeet is **competitive in Spanish accuracy**
- Parakeet is **orders of magnitude faster** than Whisper on normalized leaderboard hardware
- INT8 variant matches product need for local desktop inference

### 2. If maximum Spanish accuracy matters more than throughput, `whisper-large-v3` looks strongest

Evidence:

- Best Spanish mean from HF multilingual CSV: **3.65** vs Parakeet **3.73** vs turbo **4.34**
- Difference vs Parakeet is small, but Whisper large-v3 wins on average in Spanish benchmarks collected here

Tradeoff:

- Much slower and much heavier than current default

### 3. `whisper-large-v3-turbo` is best read as latency/size optimization, not best Spanish model

Evidence:

- Official HF card itself frames turbo as **faster with minor quality degradation**
- HF multilingual Spanish rows show turbo trailing both `large-v3` and Parakeet on average
- Biggest concern: weaker `es_covost` result

### 4. For Spanish recordings specifically, model choice depends on product goal

- **Best balance for local desktop app:** `parakeet-tdt-0.6b-v3-int8`
- **Best accuracy-first Spanish option among models already present in repo:** `whisper-large-v3` if supported operationally
- **Best low-latency Whisper option:** `whisper-large-v3-turbo`

## Suggested decision

If goal is **default model for general meeting recordings, including Spanish**, keep:

- `parakeet-tdt-0.6b-v3-int8`

Reason:

- Spanish quality is close to best observed option
- Throughput advantage is massive
- Memory footprint is better aligned with local desktop deployment

If goal shifts to **premium accuracy mode for Spanish-heavy users**, consider exposing:

- `whisper-large-v3` as explicit **accuracy mode**
- keep `parakeet-tdt-0.6b-v3-int8` as **default/speed mode**

## Caveats

- HF leaderboard data is strong, but exact deployment hardware in this app differs
- Repo default uses **INT8 Parakeet**, while HF leaderboard row is for `nvidia/parakeet-tdt-0.6b-v3`; quality should be close, but not assumed identical without in-app testing
- Spanish real-world meeting audio may differ from FLEURS / MLS / CoVoST in accents, crosstalk, jargon, and background noise
- Repo also contains `parakeet-ctc-es-0.6b-int8` beta Spanish model, but I found no equally strong public benchmark trail for it in the sources above, so I am not recommending it yet

## Best next step

Run an app-specific bakeoff on real Spanish meeting recordings:

- `parakeet-tdt-0.6b-v3-int8`
- `whisper-large-v3-turbo`
- `whisper-large-v3`
- optionally `parakeet-ctc-es-0.6b-int8`

Measure:

- WER / CER on internal gold transcripts
- latency per real-time minute
- speaker-overlap failure rate
- hallucination rate
- punctuation quality
- jargon / proper noun accuracy

Public leaderboard says enough for direction. Final default should still be validated on your own audio.