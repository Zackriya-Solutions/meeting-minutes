#!/usr/bin/env python3
"""
ASR Benchmark Tool for Meetily (Wave 15 PR-45a)

Evaluates Whisper model variants on Chinese (and optionally English)
speech datasets, computing Character Error Rate (CER), Word Error Rate
(WER), and Real-Time Factor (RTF). Results feed into the model
selection decision for the default Whisper model in the meetily UI.

Usage:
    python benchmark.py \
        --binary ./whisper.cpp/build/bin/whisper-cli \
        --models-dir ./models \
        --dataset aishell \
        --dataset-root ./data/aishell \
        --models large-v3 large-v3-turbo large-v3-turbo-q5_0 \
        --output results.json

See docs/asr_benchmark_zh.md for the full methodology and the
pre-collected public benchmark numbers used to seed the report.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import wave
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Iterable


@dataclass
class SampleResult:
    sample_id: str
    audio_path: str
    reference: str
    hypothesis: str
    cer: float
    audio_seconds: float
    inference_seconds: float
    rtf: float


@dataclass
class ModelResult:
    model_name: str
    language: str
    samples: list[SampleResult] = field(default_factory=list)

    @property
    def mean_cer(self) -> float:
        return mean(s.cer for s in self.samples) if self.samples else 0.0

    @property
    def mean_rtf(self) -> float:
        return mean(s.rtf for s in self.samples) if self.samples else 0.0

    @property
    def total_audio_seconds(self) -> float:
        return sum(s.audio_seconds for s in self.samples)

    @property
    def total_inference_seconds(self) -> float:
        return sum(s.inference_seconds for s in self.samples)


def mean(xs: Iterable[float]) -> float:
    xs = list(xs)
    return sum(xs) / len(xs) if xs else 0.0


def _levenshtein_chars(a: str, b: str) -> int:
    """Pure-Python character-level Levenshtein distance (fallback path)."""
    if a == b:
        return 0
    if not a:
        return len(b)
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cost = 0 if ca == cb else 1
            cur.append(min(cur[-1] + 1, prev[j] + 1, prev[j - 1] + cost))
        prev = cur
    return prev[-1]


def _levenshtein_tokens(a: list, b: list) -> int:
    """Pure-Python token-level Levenshtein distance (fallback path)."""
    m, n = len(a), len(b)
    if m == 0:
        return n
    if n == 0:
        return m
    prev = list(range(n + 1))
    for i in range(1, m + 1):
        cur = [i] + [0] * n
        for j in range(1, n + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            cur[j] = min(cur[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost)
        prev = cur
    return prev[n]


try:
    import Levenshtein  # type: ignore

    def edit_distance(a: str, b: str) -> int:
        return Levenshtein.distance(a, b)

    def token_edit_distance(a, b):
        return Levenshtein.distance(list(a), list(b))

except ImportError:
    def edit_distance(a: str, b: str) -> int:
        return _levenshtein_chars(a, b)

    def token_edit_distance(a, b):
        return _levenshtein_tokens(a, b)


def cer(reference: str, hypothesis: str) -> float:
    """Character Error Rate. Returns 0.0 for empty reference."""
    ref = reference.strip()
    hyp = hypothesis.strip()
    if not ref:
        return 0.0
    return edit_distance(ref, hyp) / len(ref)


def wer(reference: str, hypothesis: str) -> float:
    """Word Error Rate (space-tokenized). Useful for English samples."""
    ref = reference.strip().split()
    hyp = hypothesis.strip().split()
    if not ref:
        return 0.0
    return token_edit_distance(ref, hyp) / len(ref)


def probe_duration(wav: Path) -> float:
    """Read WAV duration via the stdlib wave module."""
    try:
        with wave.open(str(wav), "rb") as fh:
            frames = fh.getnframes()
            rate = fh.getframerate()
            return frames / float(rate) if rate else 0.0
    except (wave.Error, EOFError, FileNotFoundError):
        return 0.0


def transcribe(binary: Path, model_path: Path, audio_path: Path, language: str) -> tuple:
    """Invoke whisper.cpp, return (text, inference_seconds).

    whisper.cpp writes a sibling .txt file when -otxt -of are passed.
    We delete any stale output first, then read what whisper.cpp wrote.
    """
    out_base = audio_path.with_suffix("")
    expected_txt = out_base.with_suffix(".txt")
    if expected_txt.exists():
        expected_txt.unlink()

    cmd = [
        str(binary),
        "-m", str(model_path),
        "-f", str(audio_path),
        "-l", language,
        "-otxt",
        "-of", str(out_base),
        "-nt",
        "-np",
    ]
    start = time.perf_counter()
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elapsed = time.perf_counter() - start
    text = expected_txt.read_text(encoding="utf-8", errors="replace").strip() if expected_txt.exists() else ""
    return text, elapsed


def iter_aishell(root: Path):
    """Yield (sample_id, wav_path, reference_text) for AISHELL-1 test set."""
    trans = root / "test" / "trans.txt"
    wav_root = root / "test" / "wav"
    with trans.open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            sample_id, text = line.split(" ", 1)
            wav = wav_root / sample_id[:7] / f"{sample_id}.wav"
            if wav.exists():
                yield sample_id, wav, text


DATASETS = {
    "aishell": iter_aishell,
}


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", required=True, type=Path, help="Path to whisper-cli (or main) binary")
    parser.add_argument("--models-dir", required=True, type=Path, help="Directory containing ggml-*.bin files")
    parser.add_argument("--dataset", required=True, choices=list(DATASETS))
    parser.add_argument("--dataset-root", required=True, type=Path)
    parser.add_argument("--models", nargs="+", required=True, help="Model basenames (e.g. large-v3 large-v3-turbo)")
    parser.add_argument("--language", default="zh", help="Language code passed to whisper (-l)")
    parser.add_argument("--max-samples", type=int, default=None, help="Optional cap on sample count for smoke runs")
    parser.add_argument("--output", type=Path, default=Path("results.json"))
    args = parser.parse_args(argv)

    samples = list(DATASETS[args.dataset](args.dataset_root))
    if args.max_samples is not None:
        samples = samples[: args.max_samples]
    if not samples:
        print(f"No samples found under {args.dataset_root}", file=sys.stderr)
        return 2

    print(f"Benchmarking {len(args.models)} model(s) on {len(samples)} sample(s) ({args.dataset})", file=sys.stderr)

    results = []
    for model_name in args.models:
        model_path = args.models_dir / f"ggml-{model_name}.bin"
        if not model_path.exists():
            print(f"SKIP {model_name}: model file not found at {model_path}", file=sys.stderr)
            continue

        model_result = ModelResult(model_name=model_name, language=args.language)
        for sample_id, wav, ref in samples:
            try:
                hyp, inference_seconds = transcribe(args.binary, model_path, wav, args.language)
            except subprocess.CalledProcessError as exc:
                print(f"  {sample_id}: FAILED ({exc})", file=sys.stderr)
                continue
            audio_seconds = probe_duration(wav)
            sample = SampleResult(
                sample_id=sample_id,
                audio_path=str(wav),
                reference=ref,
                hypothesis=hyp,
                cer=cer(ref, hyp),
                audio_seconds=audio_seconds,
                inference_seconds=inference_seconds,
                rtf=inference_seconds / audio_seconds if audio_seconds > 0 else 0.0,
            )
            model_result.samples.append(sample)
            print(f"  {model_name} / {sample_id}: CER={sample.cer:.3f} RTF={sample.rtf:.2f}", file=sys.stderr)

        results.append(model_result)
        print(f"=> {model_name}: mean CER={model_result.mean_cer:.3f} mean RTF={model_result.mean_rtf:.2f}", file=sys.stderr)

    args.output.write_text(json.dumps([asdict(r) for r in results], ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Wrote {args.output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
