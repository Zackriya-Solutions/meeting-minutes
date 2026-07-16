#!/usr/bin/env python3

import json
import sqlite3
import tempfile
from pathlib import Path

from prepare_standup_corpus import (
    candidate_score,
    flatten_standup,
    generation_identity,
    read_transcription_quality,
    reviewed_references,
    select_samples,
    split_for_series,
    timestamp,
)


assert timestamp(62.9) == "[01:02]"
assert split_for_series("daily-team") == split_for_series("daily-team")

standup_score, standup_reasons = candidate_score(
    "2026-07-15_11-00_standup",
    "2026-07-15T11:00:00",
    "Сегодня закончил задачу, блокеров нет, дальше тестирование",
    20 * 60,
)
other_score, _ = candidate_score(
    "2026-07-15_17-30_one-to-one",
    "2026-07-15T17:30:00",
    "Обсудили карьерный план",
    60 * 60,
)
assert standup_score > other_score
assert "title contains standup" in standup_reasons

records = flatten_standup(
    {
        "participant_updates": [
            {
                "participant": "Анна",
                "next": [
                    {
                        "text": "Проверить сборку",
                        "evidence": [{"timestamp": "[01:02]"}],
                    }
                ],
            }
        ]
    }
)
assert records == [
    {
        "kind": "participant_update",
        "match_id": None,
        "text": "Проверить сборку",
        "owner": None,
        "due_date": None,
        "participant": "Анна",
        "category": "next",
        "evidence": [{"timestamp": "[01:02]"}],
    }
]

candidates = [
    (20, {"annotation_source": {"meeting_id": "m1"}}),
    (10, {"annotation_source": {"meeting_id": "m2"}}),
]
assert [row["annotation_source"]["meeting_id"] for row in select_samples(candidates, ["m2", "m1"], 1)] == ["m2", "m1"]
try:
    select_samples(candidates, ["missing"], 15)
    raise AssertionError("missing explicit ID must fail")
except ValueError:
    pass

db = sqlite3.connect(":memory:")
db.execute(
    "CREATE TABLE standup_records("
    "id INTEGER PRIMARY KEY, meeting_id TEXT, kind TEXT, payload TEXT, "
    "reviewed_payload TEXT, review_status TEXT)"
)
db.executemany(
    "INSERT INTO standup_records VALUES(?, 'm1', 'action', ?, ?, ?)",
    [
        (1, '{"task":"raw"}', '{"task":"human correction"}', "accepted"),
        (2, '{"task":"bad model claim"}', None, "rejected"),
        (3, '{"task":"unreviewed"}', None, "pending"),
    ],
)
assert reviewed_references(db, "m1") == [
    {
        "id": "review-1",
        "kind": "action",
        "text": "human correction",
        "owner": None,
        "due_date": None,
    }
]

identity = generation_identity(
    {
        "summary_generation": {
            "source": {
                "model_provider": "deepseek",
                "model_name": "deepseek-chat",
                "template_fingerprint": "abc:3",
                "custom_openai_endpoint": "https://secret.invalid",
            }
        }
    }
)
assert identity == ("deepseek", "deepseek-chat", "abc:3")
assert "secret" not in repr(identity)

legacy_identity = generation_identity(
    {
        "english_cache": {
            "source": {
                "model_provider": "builtin-ai",
                "model_name": "qwen3.5:4b",
                "template_fingerprint": "legacy:1",
            }
        }
    }
)
assert legacy_identity == ("builtin-ai", "qwen3.5:4b", "legacy:1")

with tempfile.TemporaryDirectory() as folder:
    Path(folder, "metadata.json").write_text(
        json.dumps(
            {
                "source_filename": "private-name.mp3",
                "source_sha256": "secret-hash",
                "transcription_quality": {
                    "processable_segments": 10,
                    "transcribed_segments": 8,
                    "empty_segments": 2,
                    "coverage_ratio": 0.8,
                    "average_confidence": None,
                    "confidence_source": "unavailable",
                    "private_extra": "must-not-leak",
                },
            }
        ),
        encoding="utf-8",
    )
    quality = read_transcription_quality(folder)
    assert quality == {
        "processable_segments": 10,
        "transcribed_segments": 8,
        "empty_segments": 2,
        "coverage_ratio": 0.8,
        "average_confidence": None,
        "confidence_source": "unavailable",
    }
    assert "private-name" not in repr(quality)
    assert "secret-hash" not in repr(quality)
    assert "must-not-leak" not in repr(quality)

print("ok - private corpus exporter helpers")
