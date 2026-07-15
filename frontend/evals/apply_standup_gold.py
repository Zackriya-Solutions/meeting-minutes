#!/usr/bin/env python3
"""Apply a private, independently reviewed gold overlay to a standup corpus export.

The provider export remains the hypothesis source. This tool only replaces reference labels and
reviewed series metadata, preventing human corrections from leaking into hypothesis records.
It prints counts and identifiers only; meeting text and labels are never written to stdout.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any

ALLOWED_SPLITS = {"train", "dev", "test"}
ALLOWED_KINDS = {
    "participant_update",
    "decision",
    "action",
    "risk",
    "deep_dive",
    "unattributed_fact",
}


def load_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} must contain a JSON object")
    return value


def validate_reference(record: Any, meeting_id: str, index: int) -> dict[str, Any]:
    if not isinstance(record, dict):
        raise ValueError(f"{meeting_id} reference {index} must be an object")
    kind = record.get("kind")
    text = record.get("text")
    record_id = record.get("id")
    if not isinstance(record_id, str) or not record_id:
        raise ValueError(f"{meeting_id} reference {index} requires a non-empty id")
    if kind not in ALLOWED_KINDS:
        raise ValueError(f"{meeting_id} reference {index} has invalid kind")
    if not isinstance(text, str) or not text.strip():
        raise ValueError(f"{meeting_id} reference {index} has empty text")
    if len(text) > 4_000:
        raise ValueError(f"{meeting_id} reference {index} text is too long")
    for optional in ("owner", "due_date"):
        value = record.get(optional)
        if value is not None and not isinstance(value, str):
            raise ValueError(f"{meeting_id} reference {index} has invalid {optional}")
    return record


def gold_by_meeting(gold: dict[str, Any]) -> dict[str, dict[str, Any]]:
    if gold.get("schema_version") != "standup_gold_v1":
        raise ValueError("unsupported gold schema_version")
    meetings = gold.get("meetings")
    if not isinstance(meetings, list) or not meetings:
        raise ValueError("gold meetings must be a non-empty array")
    indexed: dict[str, dict[str, Any]] = {}
    split_by_series: dict[str, str] = {}
    for entry in meetings:
        if not isinstance(entry, dict):
            raise ValueError("gold meeting entry must be an object")
        meeting_id = entry.get("meeting_id")
        if not isinstance(meeting_id, str) or not meeting_id:
            raise ValueError("gold meeting_id must be non-empty")
        if meeting_id in indexed:
            raise ValueError(f"duplicate gold meeting_id: {meeting_id}")
        series_id = entry.get("series_id")
        split = entry.get("split")
        if not isinstance(series_id, str) or not series_id or series_id == "UNASSIGNED":
            raise ValueError(f"{meeting_id} requires a reviewed series_id")
        if split not in ALLOWED_SPLITS:
            raise ValueError(f"{meeting_id} requires train/dev/test split")
        previous_split = split_by_series.setdefault(series_id, split)
        if previous_split != split:
            raise ValueError(f"series {series_id} appears in multiple splits")
        references = entry.get("reference_records")
        if not isinstance(references, list):
            raise ValueError(f"{meeting_id} reference_records must be an array")
        entry["reference_records"] = [
            validate_reference(record, meeting_id, index)
            for index, record in enumerate(references)
        ]
        reference_ids = [record["id"] for record in entry["reference_records"]]
        if len(reference_ids) != len(set(reference_ids)):
            raise ValueError(f"{meeting_id} contains duplicate reference IDs")
        indexed[meeting_id] = entry
    return indexed


def apply_gold(dataset: dict[str, Any], gold: dict[str, Any]) -> tuple[dict[str, Any], int]:
    samples = dataset.get("standup")
    if not isinstance(samples, list):
        raise ValueError("dataset standup must be an array")
    overlay = gold_by_meeting(gold)
    seen: set[str] = set()
    for sample in samples:
        if not isinstance(sample, dict):
            raise ValueError("dataset sample must be an object")
        source = sample.get("annotation_source") or {}
        meeting_id = source.get("meeting_id")
        if meeting_id not in overlay:
            continue
        entry = overlay[meeting_id]
        sample["series_id"] = entry["series_id"]
        sample["series_names"] = entry.get("series_names") or []
        sample["split"] = entry["split"]
        sample["reference_records"] = entry["reference_records"]
        sample["review_state"] = "manual_gold_complete"
        seen.add(meeting_id)
    missing = sorted(set(overlay) - seen)
    if missing:
        raise ValueError(f"gold meeting IDs are absent from dataset: {', '.join(missing)}")
    dataset["gold_schema_version"] = gold["schema_version"]
    dataset["gold_annotation_policy"] = gold.get("annotation_policy")
    return dataset, len(seen)


def atomic_private_write(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(f"{path.suffix}.tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    os.chmod(temporary, 0o600)
    temporary.replace(path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--gold", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    input_paths = {args.dataset.resolve(), args.gold.resolve()}
    if args.output.resolve() in input_paths:
        parser.error("output must differ from dataset and gold inputs")
    dataset = load_object(args.dataset)
    gold = load_object(args.gold)
    annotated, count = apply_gold(dataset, gold)
    atomic_private_write(args.output, annotated)
    print(f"Applied reviewed gold metadata to {count} meeting(s).")


if __name__ == "__main__":
    main()
