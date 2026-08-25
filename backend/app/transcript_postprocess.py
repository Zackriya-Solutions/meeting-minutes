"""LLM-based transcript postprocess (Wave 12 PR-42-i).

Corrects ASR output via LLM (Claude / GPT / Groq / Ollama):
- Homophones (zh/ch, n/l, de/di/de)
- Punctuation insertion (Chinese full-width)
- Paragraph breaks (\\n\\n separators)
- Known proper nouns (via custom_hotwords)
- Preserves [Silence] markers and other special tokens

Design principles:
- Pure postprocess: never mutates [Silence] / speaker tags / timestamps.
- Graceful degradation: any LLM failure returns the original text.
- Provider-pluggable: same prompt works across Claude / GPT / Groq / Ollama.
- Test-friendly: accepts an injected client for unit tests.
"""
from __future__ import annotations

import logging
import asyncio
import os
import re
from dataclasses import dataclass, field
from typing import Callable, Iterable, List, Literal, Optional

logger = logging.getLogger(__name__)

Provider = Literal["claude", "groq", "openai", "ollama"]


# ---------- config ----------


@dataclass
class PostprocessConfig:
    """Configuration for a single postprocess run."""
    enabled: bool = True
    provider: Provider = "claude"
    model_name: str = "claude-3-5-sonnet-latest"
    chunk_size: int = 4000
    overlap: int = 200
    custom_hotwords: List[str] = field(default_factory=list)

    @classmethod
    def from_env(cls) -> "PostprocessConfig":
        """Build a config from MEETILY_POSTPROCESS_* env vars (gracefully)."""
        try:
            chunk_size = int(os.environ.get("MEETILY_POSTPROCESS_CHUNK_SIZE", "4000"))
            overlap = int(os.environ.get("MEETILY_POSTPROCESS_OVERLAP", "200"))
        except ValueError:
            chunk_size, overlap = 4000, 200
        hotwords_env = os.environ.get("MEETILY_POSTPROCESS_HOTWORDS", "")
        hotwords = [w.strip() for w in hotwords_env.split(",") if w.strip()]
        return cls(
            enabled=os.environ.get("MEETILY_POSTPROCESS_DISABLED", "0") != "1",
            provider=os.environ.get("MEETILY_POSTPROCESS_PROVIDER", "claude"),  # type: ignore[arg-type]
            model_name=os.environ.get("MEETILY_POSTPROCESS_MODEL", "claude-3-5-sonnet-latest"),
            chunk_size=chunk_size,
            overlap=overlap,
            custom_hotwords=hotwords,
        )


# ---------- prompt templates ----------


SYSTEM_PROMPT = (
    "You are a senior Chinese transcript editor. Your task is to correct and "
    "format the meeting transcript produced by ASR (speech-to-text). "
    "Apply ONLY these corrections:\n"
    "1. Fix homophone errors (e.g., \u4f5c\u6218/\u5750\u573a, \u54ea\u91cc/\u6781\u7535, "
    "\u7684/\u5730/\u5f97) based on context.\n"
    "2. Add Chinese full-width punctuation: \uff0c (comma), \u3002 (period), "
    "\uff1f (question), \uff01 (exclamation), \uff1b (semicolon), "
    "\uff1a (colon), \u201c\u201d (quotes).\n"
    "3. Insert blank lines (\\n\\n) between distinct speaker turns or topic "
    "changes. Keep utterances within a single turn together.\n"
    "4. Preserve the original meaning exactly; do NOT change substantive "
    "content, decisions, action items, or facts.\n"
    "5. Capitalize / correct known person / company / place names when context "
    "makes them unambiguous.\n"
    "6. Preserve special markers VERBATIM: [Silence], [Music], speaker labels "
    "(e.g., Speaker 1:), timestamps, and any non-prose tokens.\n"
    "7. Do NOT add commentary, explanation, or meta-text.\n"
    "8. If the input is already correct, output it unchanged.\n"
    "Output ONLY the corrected transcript text."
)

USER_TEMPLATE = "{transcript}"


def build_user_prompt(text: str, hotwords: Iterable[str] = ()) -> str:
    """Build the user prompt. Hotwords get appended as a hint when provided."""
    base = USER_TEMPLATE.format(transcript=text)
    hw_list = [w for w in (w.strip() for w in hotwords) if w]
    if not hw_list:
        return base
    hint = (
        "\n\n\u8bf4\u660e\uff1a\u4ee5\u4e0b\u540d\u8bcd\u5728\u4f1a\u8bae\u4e2d\u51fa\u73b0\u9891\u7387\u8f83\u9ad8\uff0c"
        "\u8bf7\u4f18\u5148\u4f7f\u7528\u8fd9\u4e9b\u62fc\u5199\uff1a\n"
        + ", ".join(hw_list)
    )
    return base + hint


# ---------- chunking ----------


def chunk_text(text: str, chunk_size: int, overlap: int) -> List[str]:
    """Split text into overlapping chunks respecting paragraph boundaries.

    Tries to break on blank lines (\\n\\n); falls back to a character-level
    sliding window for over-long paragraphs. Every returned chunk satisfies
    len(c) <= chunk_size.
    """
    if chunk_size <= 0:
        raise ValueError("chunk_size must be positive")
    if overlap < 0 or overlap >= chunk_size:
        overlap = max(0, chunk_size // 2)
    if len(text) <= chunk_size:
        return [text]

    paragraphs = text.split("\\n\\n")
    chunks: List[str] = []
    current = ""

    def _flush() -> None:
        nonlocal current
        if current:
            chunks.append(current)
            current = ""

    for para in paragraphs:
        # Over-long paragraph: hard split, then move on.
        if len(para) > chunk_size:
            _flush()
            start = 0
            step = chunk_size - overlap
            while start < len(para):
                end = min(start + chunk_size, len(para))
                chunks.append(para[start:end])
                if end >= len(para):
                    break
                start += step
            continue

        candidate = (current + "\\n\\n" + para) if current else para
        if len(candidate) <= chunk_size:
            current = candidate
        else:
            _flush()
            tail = current[-overlap:] if overlap > 0 else ""
            current = (tail + "\\n\\n" + para) if tail else para
            if len(current) > chunk_size:
                # extremely large overlap: emit empty and let next iter handle
                _flush()
                current = para
    _flush()
    return chunks

# ---------- core postprocess ----------


# A client callable takes (system: str, user: str) and returns the assistant text.
ClientCallable = Callable[[str, str], str]


class TranscriptPostprocessor:
    """Apply LLM-based correction to an ASR transcript.

    The default client_factory builds a pydantic-ai Agent for the configured
    provider. Tests inject a fake client to avoid network calls.
    """

    def __init__(
        self,
        config: Optional[PostprocessConfig] = None,
        client: Optional[ClientCallable] = None,
    ) -> None:
        self.config = config or PostprocessConfig.from_env()
        self._client = client  # if provided, skip provider wiring
        self._agent = None

    def is_available(self) -> bool:
        """True when postprocess can run (enabled + a usable client)."""
        if not self.config.enabled:
            return False
        if self._client is not None:
            return True
        try:
            self._ensure_agent()
            return self._agent is not None
        except Exception as exc:  # pragma: no cover -- network/provider dependent
            logger.warning("postprocess: agent unavailable (%s)", exc)
            return False

    def _ensure_agent(self):  # pragma: no cover -- provider wiring
        """Lazy-build a pydantic-ai Agent. Imported lazily to keep the module
        importable in environments without pydantic-ai."""
        if self._agent is not None or self._client is not None:
            return
        from pydantic_ai import Agent  # type: ignore

        provider = self.config.provider
        model_name = self.config.model_name
        if provider == "claude":
            from pydantic_ai.models.anthropic import AnthropicModel
            self._agent = Agent(AnthropicModel(model_name), system_prompt=SYSTEM_PROMPT)
        elif provider == "groq":
            from pydantic_ai.models.groq import GroqModel
            self._agent = Agent(GroqModel(model_name), system_prompt=SYSTEM_PROMPT)
        elif provider == "openai":
            from pydantic_ai.models.openai import OpenAIModel
            self._agent = Agent(OpenAIModel(model_name), system_prompt=SYSTEM_PROMPT)
        elif provider == "ollama":
            # Ollama runs locally; uses the OpenAI-compatible surface.
            from pydantic_ai.models.openai import OpenAIModel
            from pydantic_ai.providers.openai import OpenAIProvider
            self._agent = Agent(
                OpenAIModel(model_name, provider=OpenAIProvider(base_url="http://localhost:11434/v1")),
                system_prompt=SYSTEM_PROMPT,
            )
        else:
            raise ValueError(f"unknown postprocess provider: {provider}")

    async def postprocess(self, text: str) -> str:
        """Run the full pipeline. Returns the original text on any failure."""
        if not text or not text.strip():
            return text
        if not self.config.enabled:
            return text
        if not self.is_available():
            logger.debug("postprocess: unavailable, returning original text")
            return text

        chunks = chunk_text(text, self.config.chunk_size, self.config.overlap)
        if len(chunks) == 1:
            return await self._correct_one(chunks[0])

        # multi-chunk: stitch results, dropping the overlap tail of each chunk
        results: List[str] = []
        prev_tail = ""
        for idx, chunk in enumerate(chunks):
            corrected = await self._correct_one(chunk)
            if idx == 0:
                results.append(corrected)
                prev_tail = corrected[-self.config.overlap :] if self.config.overlap > 0 else ""
            else:
                # skip leading overlap-equivalent portion of corrected chunk
                if self.config.overlap > 0 and prev_tail and corrected.startswith(prev_tail):
                    corrected = corrected[len(prev_tail) :]
                results.append(corrected)
        return "\n\n".join(r for r in results if r)

    async def _correct_one(self, chunk: str) -> str:
        user_prompt = build_user_prompt(chunk, self.config.custom_hotwords)
        try:
            if self._client is not None:
                raw = self._client(SYSTEM_PROMPT, user_prompt)
                if asyncio.iscoroutine(raw):
                    raw = await raw
                corrected = raw
            else:
                assert self._agent is not None
                run = await self._agent.run(user_prompt)
                corrected = _extract_text(run)
        except Exception as exc:
            logger.warning("postprocess: chunk correction failed (%s); keeping original", exc)
            return chunk
        corrected = (corrected or "").strip()
        if not corrected:
            return chunk
        return corrected


def _extract_text(run) -> str:
    """Extract plain text from a pydantic-ai Agent run result.

    pydantic-ai exposes the text on .data (string / structured) or .output
    depending on version. Handle both.
    """
    if hasattr(run, "output"):
        return str(run.output)
    if hasattr(run, "data"):
        return str(run.data)
    return str(run)


# ---------- module-level convenience ----------


async def postprocess_transcript(
    text: str,
    config: Optional[PostprocessConfig] = None,
    client: Optional[ClientCallable] = None,
) -> str:
    """Top-level helper. Returns the postprocessed text or the original on failure."""
    svc = TranscriptPostprocessor(config=config, client=client)
    return await svc.postprocess(text)


__all__ = [
    "PostprocessConfig",
    "TranscriptPostprocessor",
    "postprocess_transcript",
    "chunk_text",
    "build_user_prompt",
    "SYSTEM_PROMPT",
]
