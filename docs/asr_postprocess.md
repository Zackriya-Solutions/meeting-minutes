# Transcript Postprocess (Wave 12 PR-42-i)

## Overview

`backend/app/transcript_postprocess.py` provides LLM-based correction of ASR
output. Addresses systematic errors in Chinese-meeting transcripts:

| Error class | Example (raw -> corrected) |
|---|---|
| Homophones | 作战 / 坐场 -> 作战 |
| Missing punctuation | wall-of-text -> 。。。 |
| Run-on paragraphs | merged utterances -> blank-line separated |
| Proper nouns | garbled company names -> canonical |

## Usage

```python
from transcript_postprocess import PostprocessConfig, TranscriptPostprocessor

cfg = PostprocessConfig(
    provider="claude",
    model_name="claude-3-5-sonnet-latest",
    chunk_size=4000,
    overlap=200,
    custom_hotwords=["Acme", "ProjectX"],
)
svc = TranscriptPostprocessor(config=cfg)
corrected = await svc.postprocess(raw_text)
```

Or top-level:

```python
from transcript_postprocess import postprocess_transcript
corrected = await postprocess_transcript(raw_text, config=cfg)
```

## Configuration

### PostprocessConfig fields

| Field | Default | Purpose |
|---|---|---|
| `enabled` | True | master switch; `False` skips postprocess entirely |
| `provider` | `"claude"` | `claude | groq | openai | ollama` |
| `model_name` | `"claude-3-5-sonnet-latest"` | provider-specific model id |
| `chunk_size` | 4000 | max chars per LLM call |
| `overlap` | 200 | overlap chars across chunks (helps continuity) |
| `custom_hotwords` | `[]` | high-priority terms to fix |

### Environment variables (alternative)

| Variable | Default | Maps to |
|---|---|---|
| `MEETILY_POSTPROCESS_DISABLED` | 0 | `enabled` (inverted) |
| `MEETILY_POSTPROCESS_PROVIDER` | claude | `provider` |
| `MEETILY_POSTPROCESS_MODEL` | claude-3-5-sonnet-latest | `model_name` |
| `MEETILY_POSTPROCESS_CHUNK_SIZE` | 4000 | `chunk_size` |
| `MEETILY_POSTPROCESS_OVERLAP` | 200 | `overlap` |
| `MEETILY_POSTPROCESS_HOTWORDS` | "" | `custom_hotwords` (comma-separated) |

## Providers

### Claude (default)
Best Chinese quality. Requires `ANTHROPIC_API_KEY` env var.

```python
PostprocessConfig(provider="claude", model_name="claude-3-5-sonnet-latest")
```

### Groq
Fast + cheap. Requires `GROQ_API_KEY`.

```python
PostprocessConfig(provider="groq", model_name="llama-3.3-70b-versatile")
```

### OpenAI
Mid quality for Chinese. Requires `OPENAI_API_KEY`.

```python
PostprocessConfig(provider="openai", model_name="gpt-4o-mini")
```

### Ollama (local, privacy-first)
Runs against `http://localhost:11434/v1`. No API key required.

```python
PostprocessConfig(provider="ollama", model_name="qwen2.5:7b")
```

## Prompt Design

The module ships a fixed system prompt (see `SYSTEM_PROMPT`). It enforces:

1. Homophone correction based on context
2. Chinese full-width punctuation (。 ， ？ ！ ： ；)
3. Blank-line paragraph breaks
4. Preserve meaning -- no content invention
5. Fix known proper nouns when unambiguous
6. Preserve [Silence], speaker labels, timestamps, other markers verbatim
7. Output ONLY the corrected text

Custom hotwords are appended to the user prompt as a "high-priority terms"
hint when provided.

## Graceful Degradation

| Scenario | Behavior |
|---|---|
| `enabled=False` | returns original text |
| Empty / whitespace text | returns as-is |
| Provider API key missing | logs warning, returns original text |
| LLM call raises | logs warning, returns the original chunk |
| LLM returns empty | returns the original chunk |
| No pydantic-ai installed | `is_available()` returns False, `postprocess()` returns original |

The module never raises to the caller; the worst case is "no correction".

## Testing

```bash
python -c "import sys; sys.path.insert(0, 'backend/app'); \
    from transcript_postprocess import *; \
    import asyncio; \
    asyncio.run(_run_all_tests())"
```

16 unit tests cover: chunking (basic / overlong / preserves / single / zero
raises), PostprocessConfig (defaults / from_env), prompt construction, sync
and async clients, failure fallback, empty passthrough, multi-chunk stitch.

## Integration Roadmap

| Step | Status |
|---|---|
| Module + prompt + tests | **this PR (PR-42-i)** |
| transcript_processor integration | PR-42-i (this commit) |
| Settings UI toggle | PR-42-ii |
| API endpoint `/postprocess-transcript` | PR-42-ii |
| End-to-end recording -> postprocessed transcript test | PR-42-ii |

## Performance

- LLM latency dominates (provider-dependent)
- Claude Sonnet: ~1-3 s per 4k-char chunk
- Groq Llama-3.3-70b: ~0.5-1 s per chunk
- chunking adds <10 ms overhead for typical meeting transcripts
- No GPU required for cloud providers; Ollama on GPU recommended for local

## References

- pydantic-ai: https://ai.pydantic.dev/
- Existing summary pipeline: backend/app/transcript_processor.py
- Spec: docs/superpowers/specs/2026-07-14-postprocess-wave12.md
