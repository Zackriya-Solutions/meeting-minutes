# Wave 12: LLM Postprocess for ASR Correction (PR-42)

> **For agentic workers:** REQUIRED SUB-KILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.
> **Base branch:** feature/stability-wave8
> **Parent waves:** Wave 10 (recognition) + Wave 11 (diarization), shipped

## Background

ASR (Whisper / Parakeet / cloud) outputs contain systematic errors in Chinese
meetings:
- **Homophones**: zh/ch (作战/坐场), n/l (哪里/极电), de/di/de (的/地/得)
- **Missing punctuation**: most ASR output is a wall of text, hard to read
- **Run-on paragraphs**: short utterances by multiple speakers merge together
- **Proper nouns**: company / product / person names frequently garbled

Wave 10 (local model + hotwords + perf optimization) and Wave 11 (diarization)
address upstream. The final correction pass needs LLM postprocessing.

## Goals

- One LLM call (Claude / GPT / Groq / Ollama) to fix ASR transcript text
- Major Chinese-meeting fixes: homophones, punctuation, paragraph breaks
- Pluggable LLM provider + prompt template
- Robust error handling that never corrupts the original transcript
- Preserve speaker / Silence / timestamp markers

## Non-Goals

- Not replacing the ASR model (postprocessing only)
- Not requiring Ollama / local LLM
- Not requiring GPU
- Not redesigning the summary pipeline
- Not mining conversation analytics

## Selection Decisions

### Prompt Strategy

| Strategy | Pros | Cons |
|---|---|---|
| Single prompt "fix + punctuate" | Simple | Mid accuracy |
| **Structured prompt** (homophone table + punctuation rules) | High accuracy | Long prompt, more tokens |
| Few-shot learning | Best accuracy | Token heavy |

**Recommended**: Homophone table + punctuation rules + 1 Few-shot example,
output forced to plain text (no JSON wrapping needed).

### LLM Provider Strategy

| Provider | Pros | Cons | Use |
|---|---|---|---|
| Claude Sonnet | Best Chinese | API key + cost | **default** |
| GPT-4o-mini | Average | cost + weaker Chinese | fallback |
| Groq Llama-3.3-70b | Fast + cheap | mid Chinese | fallback |
| Ollama (local) | Private + free | requires user setup | opt-in |

**Default Claude** (best quality, smart reasoning). Ollama supported as
opt-in fallback.

## Scope

| PR | Topic | Key Files | Commits |
|----|-------|-----------|---------|
| 42-i | Postprocess module | backend/app/transcript_postprocess.py (new) + transcript_processor integration | 1 commit |
| 42-ii | Settings UI + API endpoint | main.py + frontend/src/components/PostprocessSettings.tsx | 1 commit |

**PR-42-i scope** (this PR):
- backend/app/transcript_postprocess.py: new module
  - PostprocessConfig + PostprocessResult dataclasses
  - Prompt template (SYSTEM + USER)
  - postprocess_transcript(text, config) -> text + is_available() check
- Mocked LLM logic for tests
- 5+ unit tests (mock LLM)
- Docs

## Architecture

```
[ASR output (raw text)]
       |
       v
[transcript_postprocess.py: postprocess_transcript()]
       |-> LLM (Claude / GPT / Groq / Ollama) with correction prompt
       v
[Corrected text (same length, paragraphs preserved)]
```

### Prompt Design

SYSTEM (snippet):
You are a senior Chinese transcript editor. Your task is to correct and
format the meeting transcript produced by ASR:
1. Fix homophone errors (e.g., 作战/坐场, 哪里/极电, 的/地/得)
2. Add punctuation (commas, periods, question marks, exclamation marks)
3. Use blank lines ("\n\n") between paragraphs
4. Preserve original meaning; do NOT change substantive content
5. Fix known person / company / place names when context is clear
6. Preserve [Silence] and other special markers

USER template:
```
{transcript_text}
```

OUTPUT rules:
- Output ONLY the corrected text, no prompt echo / explanation
- Preserve original paragraph breaks
- If the text is already correct, output it unchanged

### PostprocessConfig definition

```python
@dataclass
class PostprocessConfig:
    enabled: bool = True
    provider: Literal["claude", "groq", "openai", "ollama"] = "claude"
    model_name: str = "claude-3-5-sonnet-latest"
    chunk_size: int = 4000
    overlap: int = 200
    custom_hotwords: list[str] = field(default_factory=list)
```

## Acceptance (PR-42-i)

- [ ] backend/app/transcript_postprocess.py implemented
- [ ] 5+ unit tests pass (mock LLM)
- [ ] transcript_processor.py integration (call site added, gated by config)
- [ ] Python py_compile passes
- [ ] docs/asr_postprocess.md complete

## Risks

| Risk | Mitigation |
|---|---|
| LLM corrupts output | JSON schema + truncate / retry |
| Token blow-up | chunk_size=4000 chunked |
| Privacy concerns | Ollama local fallback already supported |
| LLM introduces new errors | prompt enforces "preserve original meaning" |

## References

- pydantic-ai: https://ai.pydantic.dev/
- Claude prompt engineering: https://docs.anthropic.com/en/docs/build-with-claude/prompt-engineering/use-xml-tags
- Existing summary pipeline: backend/app/transcript_processor.py
