#!/usr/bin/env python3
"""Unit tests for scripts/asr_benchmark/benchmark.py (Wave 15 PR-45a).

Runs without whisper.cpp / GPU / dataset. Pure-python; uses the stdlib
wave module for duration probing, so no audio dependency is needed.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
import wave
from pathlib import Path

# Allow running both as `python test_benchmark.py` (from the script dir)
# and `python -m unittest scripts.asr_benchmark.test_benchmark` (from repo root).
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

import benchmark as bm  # type: ignore


class TestCER(unittest.TestCase):
    def test_perfect_match_returns_zero(self):
        self.assertEqual(bm.cer("你好世界", "你好世界"), 0.0)

    def test_single_char_substitution(self):
        # 1 substitution in 4 chars = 0.25
        self.assertAlmostEqual(bm.cer("你好世界", "你号世界"), 0.25)

    def test_single_char_insertion(self):
        # ref=3 chars, hyp=4 chars, 1 insertion -> 1/3
        self.assertAlmostEqual(bm.cer("你好世", "你好世界"), 1 / 3)

    def test_single_char_deletion(self):
        # ref=4 chars, hyp=2 chars, 2 deletions -> 2/4 = 0.5
        self.assertAlmostEqual(bm.cer("你好世界", "你世"), 0.5)

    def test_empty_reference_returns_zero(self):
        self.assertEqual(bm.cer("", ""), 0.0)
        self.assertEqual(bm.cer("", "anything"), 0.0)

    def test_whitespace_only_reference_returns_zero(self):
        self.assertEqual(bm.cer("   ", "anything"), 0.0)

    def test_completely_wrong_hypothesis(self):
        # 3 substitutions in 4 chars
        self.assertAlmostEqual(bm.cer("abcd", "wxyz"), 1.0)

    def test_chinese_long_phrase(self):
        # Realistic Chinese: small typo
        ref = "今天我们讨论项目的进展情况"
        hyp = "今天我们讨论项目的进张情况"
        self.assertGreater(bm.cer(ref, hyp), 0.0)
        self.assertLess(bm.cer(ref, hyp), 0.2)


class TestWER(unittest.TestCase):
    def test_perfect_match(self):
        self.assertEqual(bm.wer("hello world", "hello world"), 0.0)

    def test_one_word_wrong(self):
        self.assertAlmostEqual(bm.wer("hello world", "hello there"), 1 / 2)

    def test_extra_word(self):
        # ref=2, hyp=3, 1 insertion -> 0.5
        self.assertAlmostEqual(bm.wer("a b", "a b c"), 0.5)


class TestProbeDuration(unittest.TestCase):
    def test_known_duration(self):
        with tempfile.TemporaryDirectory() as tmp:
            wav = Path(tmp) / "tone.wav"
            with wave.open(str(wav), "wb") as fh:
                fh.setnchannels(1)
                fh.setsampwidth(2)
                fh.setframerate(16000)
                fh.writeframes(b"\x00\x00" * 16000)  # 1 second of silence
            self.assertAlmostEqual(bm.probe_duration(wav), 1.0, places=3)

    def test_half_second(self):
        with tempfile.TemporaryDirectory() as tmp:
            wav = Path(tmp) / "half.wav"
            with wave.open(str(wav), "wb") as fh:
                fh.setnchannels(1)
                fh.setsampwidth(2)
                fh.setframerate(16000)
                fh.writeframes(b"\x00\x00" * 8000)  # 0.5 second
            self.assertAlmostEqual(bm.probe_duration(wav), 0.5, places=3)

    def test_missing_file_returns_zero(self):
        self.assertEqual(bm.probe_duration(Path("/nonexistent/a.wav")), 0.0)


class TestModelResultAggregations(unittest.TestCase):
    def _make_result(self, samples):
        from benchmark import ModelResult, SampleResult  # type: ignore
        return ModelResult(model_name="m", language="zh", samples=samples)

    def test_empty_aggregations(self):
        r = self._make_result([])
        self.assertEqual(r.mean_cer, 0.0)
        self.assertEqual(r.mean_rtf, 0.0)
        self.assertEqual(r.total_audio_seconds, 0.0)

    def test_aggregations_with_samples(self):
        from benchmark import SampleResult  # type: ignore
        s1 = SampleResult("a", "/p/a.wav", "ref", "hyp", cer=0.1, audio_seconds=10, inference_seconds=2, rtf=0.2)
        s2 = SampleResult("b", "/p/b.wav", "ref", "hyp", cer=0.3, audio_seconds=20, inference_seconds=4, rtf=0.2)
        r = self._make_result([s1, s2])
        self.assertAlmostEqual(r.mean_cer, 0.2)
        self.assertAlmostEqual(r.mean_rtf, 0.2)
        self.assertAlmostEqual(r.total_audio_seconds, 30)
        self.assertAlmostEqual(r.total_inference_seconds, 6)


if __name__ == "__main__":
    unittest.main(verbosity=2)
