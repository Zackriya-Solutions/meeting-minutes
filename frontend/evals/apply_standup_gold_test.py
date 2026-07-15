#!/usr/bin/env python3

import tempfile
from pathlib import Path

from apply_standup_gold import (
    apply_gold,
    atomic_private_write,
    ensure_output_available,
    gold_by_meeting,
)


dataset = {
    "standup": [
        {
            "annotation_source": {"meeting_id": "meeting-1"},
            "hypothesis_records": [{"kind": "action", "text": "raw model output"}],
            "reference_records": [],
            "series_id": "UNASSIGNED",
            "split": "unassigned",
        }
    ]
}
gold = {
    "schema_version": "standup_gold_v1",
    "annotation_policy": "manual transcript review",
    "meetings": [
        {
            "meeting_id": "meeting-1",
            "series_id": "series-a",
            "series_names": ["A"],
            "split": "test",
            "meeting_type": "pure_status",
            "recording_scope": "complete",
            "reference_records": [
                {
                    "id": "gold-1",
                    "kind": "action",
                    "text": "reviewed action",
                    "owner": None,
                    "due_date": None,
                }
            ],
        }
    ],
}

result, count = apply_gold(dataset, gold)
sample = result["standup"][0]
assert count == 1
assert sample["hypothesis_records"][0]["text"] == "raw model output"
assert sample["reference_records"][0]["text"] == "reviewed action"
assert sample["series_id"] == "series-a"
assert sample["split"] == "test"
assert sample["meeting_type"] == "pure_status"
assert sample["recording_scope"] == "complete"
assert sample["review_state"] == "manual_gold_complete"

conflicting_split = {
    "schema_version": "standup_gold_v1",
    "meetings": [
        {
            "meeting_id": "meeting-1",
            "series_id": "series-a",
            "split": "train",
            "meeting_type": "pure_status",
            "recording_scope": "complete",
            "reference_records": [],
        },
        {
            "meeting_id": "meeting-2",
            "series_id": "series-a",
            "split": "test",
            "meeting_type": "pure_status",
            "recording_scope": "complete",
            "reference_records": [],
        },
    ],
}
try:
    gold_by_meeting(conflicting_split)
    raise AssertionError("a recurring series must not cross splits")
except ValueError as error:
    assert "multiple splits" in str(error)

with tempfile.TemporaryDirectory() as directory:
    output = Path(directory) / "private.json"
    atomic_private_write(output, result)
    assert output.stat().st_mode & 0o777 == 0o600
    try:
        ensure_output_available(output, overwrite=False)
        raise AssertionError("existing private output must fail closed")
    except ValueError as error:
        assert "already exists" in str(error)
    ensure_output_available(output, overwrite=True)

print("ok - private standup gold overlay")
