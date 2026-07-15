#!/usr/bin/env python3

from prepare_standup_corpus import candidate_score, flatten_standup, split_for_series, timestamp


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

print("ok - private corpus exporter helpers")
