"""Offline benchmark for meetily's streaming transcription.

Drives the nemotron-helper sidecar directly over its stdio protocol, so a change
to segmentation strategy can be scored in seconds without launching the app,
opening a microphone, or rebuilding the Tauri crate.

Four numbers come out, matching the two complaints being worked on:

  wer                accuracy against a reference transcript
  rtf                real-time factor (< 1.0 keeps up with live audio)
  first_commit_s     audio position at which the first word was committed
  max_commit_gap_s   longest stretch of audio with no committed text

The last two are what "the transcript sits still while someone talks" means
numerically. Word error rate alone cannot see that failure.

Strategies:

  stream    feed fixed-size chunks continuously, keeping the encoder cache
            across them; every chunk's output is committed as it arrives.
            This is what a cache-aware streaming model is built for.
  utterance one call per span listed in a segments JSON file, resetting between
            them. Reproduces today's VAD-driven behaviour, where nothing is
            committed until the speaker pauses.
  oneshot   a single call over the whole clip. Not a live strategy - it is the
            accuracy ceiling to compare the others against.

Usage:
  python streaming_bench.py --audio fixtures/watchshop_60s_16k.f32 \
      --reference fixtures/watchshop_60s_reference.txt \
      --strategy stream --chunk-ms 560
"""

from __future__ import annotations

import argparse
import array
import base64
import json
import os
import re
import subprocess
import sys
import time
import wave
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
SIDECAR = HERE.parent / "binaries" / "nemotron-helper-x86_64-pc-windows-msvc.exe"
MODEL_DIR = (
    Path(os.environ["APPDATA"]) / "com.meetily.ai" / "models" / "nemotron"
    / "nemotron-3.5-asr-streaming-0.6b"
)
SAMPLE_RATE = 16_000

# The sidecar rejects anything shorter than this, so the tail of a clip that does
# not divide evenly into chunks is padded rather than dropped.
MIN_SAMPLES = SAMPLE_RATE // 10


class Sidecar:
    """One nemotron-helper process, spoken to in newline-delimited JSON."""

    def __init__(self, binary: Path, model_dir: Path, language: str | None = None):
        self.proc = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
        )
        reply = self.request(
            {"type": "load", "model_dir": str(model_dir), "language": language or "auto"}
        )
        if reply.get("type") != "loaded":
            raise RuntimeError(f"sidecar refused to load the model: {reply}")
        self.provider = reply["provider"]

    def request(self, payload: dict) -> dict:
        self.proc.stdin.write((json.dumps(payload) + "\n").encode())
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("sidecar exited without replying")
        return json.loads(line)

    def transcribe(self, samples: array.array) -> str:
        payload = base64.b64encode(samples.tobytes()).decode()
        reply = self.request({"type": "transcribe", "audio_b64": payload})
        if reply.get("type") != "transcript":
            raise RuntimeError(f"unexpected reply: {reply}")
        return reply["text"]

    def transcribe_stream(self, samples: array.array) -> str:
        """One streaming step. The reply is verbatim - see Request::TranscribeStream.

        Pieces must be concatenated with nothing between them: Nemotron marks the
        start of a word with a leading space, so inserting one turns "speed" +
        "masters" into two words and dropping one glues two words together.
        """
        payload = base64.b64encode(samples.tobytes()).decode()
        reply = self.request({"type": "transcribe_stream", "audio_b64": payload})
        if reply.get("type") != "piece":
            raise RuntimeError(f"unexpected reply: {reply}")
        return reply["text"]

    def reset(self) -> None:
        self.request({"type": "reset"})

    def close(self) -> None:
        try:
            self.request({"type": "shutdown"})
        except Exception:
            pass
        self.proc.kill()
        self.proc.wait()


@dataclass
class Commit:
    """Text finalised at a point in the audio, with how long the model took."""

    audio_s: float
    text: str
    compute_s: float


@dataclass
class Run:
    strategy: str
    chunk_ms: int
    audio_s: float
    commits: list[Commit] = field(default_factory=list)
    joiner: str = " "

    @property
    def transcript(self) -> str:
        """Committed text in order.

        Streaming pieces already carry their own leading spaces, so they are
        joined with nothing. Whole-utterance strategies come back trimmed, so
        they need a separator; `joiner` records which applies.
        """
        return self.joiner.join(c.text for c in self.commits if c.text.strip())

    @property
    def compute_s(self) -> float:
        return sum(c.compute_s for c in self.commits)

    @property
    def rtf(self) -> float:
        return self.compute_s / self.audio_s if self.audio_s else float("nan")

    @property
    def first_commit_s(self) -> float | None:
        """Audio position of the first commit that actually carried words."""
        for c in self.commits:
            if c.text.strip():
                return c.audio_s
        return None

    @property
    def max_commit_gap_s(self) -> float:
        """Longest run of audio ending with no committed text.

        Measured from the start of the clip and closed at the end, so a strategy
        that commits only after the final pause is charged for the whole clip.
        """
        worst = 0.0
        previous = 0.0
        for c in self.commits:
            if not c.text.strip():
                continue
            worst = max(worst, c.audio_s - previous)
            previous = c.audio_s
        return max(worst, self.audio_s - previous)


def read_f32(path: Path) -> array.array:
    """Load audio as mono 16 kHz f32.

    Accepts a headerless f32le fixture or a WAV file. WAV matters because it is what
    the app itself writes for every recording: after a real session, its own mixed
    output can be scored here. That is the only way to check the part of the path this
    harness cannot reach - real devices, real levels, the mixer's ducking - without
    driving the UI by hand.
    """
    if path.suffix.lower() == ".wav":
        return read_wav(path)

    samples = array.array("f")
    data = path.read_bytes()
    samples.frombytes(data[: len(data) - len(data) % 4])
    return samples


def read_wav(path: Path) -> array.array:
    with wave.open(str(path), "rb") as wav:
        channels, width, rate = wav.getnchannels(), wav.getsampwidth(), wav.getframerate()
        raw = wav.readframes(wav.getnframes())

    if width == 2:
        pcm = array.array("h")
        pcm.frombytes(raw)
        samples = array.array("f", (s / 32768.0 for s in pcm))
    elif width == 4:
        # Could be f32 or s32; the app writes f32, and s32 samples read as f32 would be
        # wildly out of range, so the range is what tells them apart.
        samples = array.array("f")
        samples.frombytes(raw)
        if any(abs(s) > 8.0 for s in samples[:4096]):
            pcm = array.array("i")
            pcm.frombytes(raw)
            samples = array.array("f", (s / 2147483648.0 for s in pcm))
    else:
        raise RuntimeError(f"{path}: {width * 8}-bit WAV is not supported")

    if channels > 1:
        samples = array.array(
            "f",
            (
                sum(samples[i : i + channels]) / channels
                for i in range(0, len(samples) - channels + 1, channels)
            ),
        )

    if rate != SAMPLE_RATE:
        samples = resample_linear(samples, rate, SAMPLE_RATE)

    return samples


def resample_linear(samples: array.array, source_rate: int, target_rate: int) -> array.array:
    """Good enough to score a recording, and not what the app uses.

    The pipeline resamples with a band-limited sinc filter; this is linear
    interpolation, which adds a little aliasing. It exists so a WAV at any rate can be
    fed in, not to reproduce the app's own resampling - the Rust end-to-end test covers
    that path exactly.
    """
    ratio = source_rate / target_rate
    count = int(len(samples) / ratio)
    out = array.array("f", bytes(4 * count))
    for i in range(count):
        position = i * ratio
        left = int(position)
        right = min(left + 1, len(samples) - 1)
        weight = position - left
        out[i] = samples[left] * (1.0 - weight) + samples[right] * weight
    return out


def pad(samples: array.array) -> array.array:
    if len(samples) >= MIN_SAMPLES:
        return samples
    padded = array.array("f", samples)
    padded.extend([0.0] * (MIN_SAMPLES - len(samples)))
    return padded


def run_stream(sidecar: Sidecar, samples: array.array, chunk_ms: int) -> Run:
    """Feed the clip as a continuous stream, committing whatever each chunk emits.

    No reset between chunks: the encoder cache carrying left context across chunk
    boundaries is the whole point, and it is what keeps a cut mid-sentence from
    costing accuracy the way cutting the audio into independent pieces does.
    """
    chunk = int(SAMPLE_RATE * chunk_ms / 1000)
    run = Run("stream", chunk_ms, len(samples) / SAMPLE_RATE, joiner="")
    sidecar.reset()
    for start in range(0, len(samples), chunk):
        piece = samples[start : start + chunk]
        began = time.perf_counter()
        text = sidecar.transcribe_stream(pad(piece))
        run.commits.append(
            Commit(
                audio_s=(start + len(piece)) / SAMPLE_RATE,
                text=text,
                compute_s=time.perf_counter() - began,
            )
        )
    return run


def run_utterance(sidecar: Sidecar, samples: array.array, spans: list[tuple[float, float]]) -> Run:
    """One call per span, reset in between - today's VAD-driven behaviour."""
    run = Run("utterance", 0, len(samples) / SAMPLE_RATE)
    for start_s, end_s in spans:
        piece = samples[int(start_s * SAMPLE_RATE) : int(end_s * SAMPLE_RATE)]
        if not len(piece):
            continue
        sidecar.reset()
        began = time.perf_counter()
        text = sidecar.transcribe(pad(piece))
        run.commits.append(
            Commit(audio_s=end_s, text=text, compute_s=time.perf_counter() - began)
        )
    return run


def run_chopped(sidecar: Sidecar, samples: array.array, segment_ms: int) -> Run:
    """Cut the clip into segments of an arbitrary length and call `transcribe` per
    segment, exactly as nemotron_provider.rs does with VAD spans.

    This isolates one failure with one variable. `transcribe` loops
    `samples.chunks(8960)`, so a segment whose length is not a multiple of 560 ms
    ends with a short chunk. `transcribe_chunk` will not decode a partial step: it
    keeps the audio buffered and returns nothing, and the model's `audio_processed`
    counter now disagrees with where the caller thinks it is. Every following
    segment inherits the drift, which is why words come back with their openings
    shaved off ("speedmasters" -> "speed bam") rather than merely misspelled.

    No gaps are introduced here. Real VAD additionally drops audio outright
    (measured at 67% coverage in the prior session), so this is a *lower* bound on
    the damage the live path takes.
    """
    step = int(SAMPLE_RATE * segment_ms / 1000)
    run = Run("chopped", segment_ms, len(samples) / SAMPLE_RATE)
    for start in range(0, len(samples), step):
        piece = samples[start : start + step]
        began = time.perf_counter()
        text = sidecar.transcribe(pad(piece))
        run.commits.append(
            Commit(
                audio_s=(start + len(piece)) / SAMPLE_RATE,
                text=text,
                compute_s=time.perf_counter() - began,
            )
        )
    return run


def run_oneshot(sidecar: Sidecar, samples: array.array) -> Run:
    run = Run("oneshot", 0, len(samples) / SAMPLE_RATE)
    sidecar.reset()
    began = time.perf_counter()
    text = sidecar.transcribe(samples)
    run.commits.append(
        Commit(audio_s=run.audio_s, text=text, compute_s=time.perf_counter() - began)
    )
    return run


# Normalisation is deliberately blunt: casing, punctuation and the ">>" speaker
# markers YouTube emits carry no information about whether a word was heard.
_DROP = re.compile(r"[^a-z0-9' ]+")

# Digits and spelled-out numbers are the same words spoken aloud, and the two
# reference sources disagree on which to write. Counting "15" against "fifteen"
# would charge a strategy for its formatter rather than its hearing.
_ONES = "zero one two three four five six seven eight nine ten eleven twelve \
thirteen fourteen fifteen sixteen seventeen eighteen nineteen".split()
_TENS = {20: "twenty", 30: "thirty", 40: "forty", 50: "fifty",
         60: "sixty", 70: "seventy", 80: "eighty", 90: "ninety"}

# Casual spellings the models emit inconsistently for the same sound.
_COLLOQUIAL = {"gonna": "going to", "wanna": "want to", "gotta": "got to",
               "kinda": "kind of", "sorta": "sort of", "cuz": "because",
               "'cause": "because", "yep": "yeah", "yup": "yeah", "ok": "okay"}


def _spell(number: int) -> str:
    if number < 20:
        return _ONES[number]
    if number < 100:
        tens, ones = divmod(number, 10)
        return _TENS[tens * 10] + ("" if ones == 0 else " " + _ONES[ones])
    return str(number)


def normalise(text: str) -> list[str]:
    text = text.lower().replace(">>", " ").replace("-", " ")
    text = _DROP.sub(" ", text)
    words = []
    for word in text.split():
        word = _COLLOQUIAL.get(word, word)
        if word.isdigit() and int(word) < 100:
            word = _spell(int(word))
        words.extend(word.split())
    return words


def wer(reference: str, hypothesis: str) -> tuple[float, dict]:
    """Levenshtein word error rate, plus the edit counts behind it."""
    ref, hyp = normalise(reference), normalise(hypothesis)
    if not ref:
        return float("nan"), {}

    # Only two rows are ever needed; each cell also carries its edit tally so the
    # substitution/deletion/insertion split can be reported, not just the total.
    row = [(j, (0, 0, j)) for j in range(len(hyp) + 1)]
    for i in range(1, len(ref) + 1):
        nxt = [(i, (0, i, 0))]
        for j in range(1, len(hyp) + 1):
            if ref[i - 1] == hyp[j - 1]:
                nxt.append((row[j - 1][0], row[j - 1][1]))
                continue
            sub = (row[j - 1][0] + 1, _bump(row[j - 1][1], 0))
            dele = (row[j][0] + 1, _bump(row[j][1], 1))
            ins = (nxt[j - 1][0] + 1, _bump(nxt[j - 1][1], 2))
            nxt.append(min(sub, dele, ins, key=lambda c: c[0]))
        row = nxt

    distance, (subs, dels, ins) = row[-1]
    return distance / len(ref), {
        "substitutions": subs,
        "deletions": dels,
        "insertions": ins,
        "reference_words": len(ref),
        "hypothesis_words": len(hyp),
    }


def _bump(counts: tuple[int, int, int], index: int) -> tuple[int, int, int]:
    listed = list(counts)
    listed[index] += 1
    return tuple(listed)


def report(run: Run, reference: str) -> dict:
    rate, counts = wer(reference, run.transcript)
    first = run.first_commit_s
    return {
        "strategy": run.strategy,
        "chunk_ms": run.chunk_ms or None,
        "audio_s": round(run.audio_s, 2),
        "wer": round(rate, 4),
        "rtf": round(run.rtf, 3),
        "first_commit_s": None if first is None else round(first, 2),
        "max_commit_gap_s": round(run.max_commit_gap_s, 2),
        "commits_with_text": sum(1 for c in run.commits if c.text.strip()),
        "commits_total": len(run.commits),
        **counts,
        # When each commit landed, so a long gap can be looked at rather than guessed
        # at. A gap that lines up with silence in the audio is content; gaps that recur
        # on a fixed period are the decoder stalling.
        "timeline": [
            {"audio_s": round(c.audio_s, 2), "text": c.text}
            for c in run.commits
            if c.text.strip()
        ],
        "transcript": run.transcript,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--audio",
        required=True,
        type=Path,
        help="f32le mono 16 kHz raw, or a WAV (including one the app recorded)",
    )
    parser.add_argument("--reference", type=Path, help="reference transcript, plain text")
    parser.add_argument(
        "--strategy",
        default="stream",
        choices=["stream", "utterance", "oneshot", "chopped"],
    )
    parser.add_argument(
        "--segment-ms",
        type=int,
        action="append",
        help="chopped segment length; repeat to sweep several (default 4000)",
    )
    parser.add_argument(
        "--chunk-ms",
        type=int,
        action="append",
        help="stream chunk size; repeat to sweep several (default 560)",
    )
    parser.add_argument("--segments", type=Path, help='utterance spans: [[start_s, end_s], ...]')
    parser.add_argument("--language", default="auto")
    parser.add_argument("--json", type=Path, help="write the full report here")
    parser.add_argument("--sidecar", type=Path, default=SIDECAR)
    parser.add_argument("--model-dir", type=Path, default=MODEL_DIR)
    args = parser.parse_args()

    samples = read_f32(args.audio)
    reference = args.reference.read_text(encoding="utf-8") if args.reference else ""

    sidecar = Sidecar(args.sidecar, args.model_dir, args.language)
    print(f"sidecar loaded on {sidecar.provider}, {len(samples) / SAMPLE_RATE:.1f}s of audio\n")

    reports = []
    try:
        if args.strategy == "stream":
            for chunk_ms in args.chunk_ms or [560]:
                reports.append(report(run_stream(sidecar, samples, chunk_ms), reference))
        elif args.strategy == "chopped":
            for segment_ms in args.segment_ms or [4000]:
                reports.append(report(run_chopped(sidecar, samples, segment_ms), reference))
        elif args.strategy == "utterance":
            spans = json.loads(args.segments.read_text()) if args.segments else []
            reports.append(report(run_utterance(sidecar, samples, spans), reference))
        else:
            reports.append(report(run_oneshot(sidecar, samples), reference))
    finally:
        sidecar.close()

    header = f"{'strategy':<12} {'chunk':>7} {'WER':>8} {'RTF':>7} {'1st':>7} {'maxgap':>7}"
    print(header)
    print("-" * len(header))
    for r in reports:
        chunk = f"{r['chunk_ms']}ms" if r["chunk_ms"] else "-"
        first = "never" if r["first_commit_s"] is None else f"{r['first_commit_s']:.1f}s"
        wer_cell = "-" if r["wer"] != r["wer"] else f"{r['wer'] * 100:.1f}%"
        print(
            f"{r['strategy']:<12} {chunk:>7} {wer_cell:>8} {r['rtf']:>7.2f} "
            f"{first:>7} {r['max_commit_gap_s']:>6.1f}s"
        )

    for r in reports:
        print(f"\n--- {r['strategy']} {r['chunk_ms'] or ''} ---\n{r['transcript']}")

    if args.json:
        args.json.write_text(json.dumps(reports, indent=2), encoding="utf-8")
        print(f"\nwrote {args.json}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
