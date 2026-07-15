#!/usr/bin/env python3
"""Apply independently reviewed hypothesis-to-gold links to a private standup export.

Links bind to an exact raw hypothesis fingerprint. A provider rerun therefore invalidates stale
links instead of silently transferring them to changed output. Meeting text is never printed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

from apply_standup_gold import atomic_private_write, ensure_output_available, load_object

FINGERPRINT_FIELDS = ("kind", "text", "participant", "owner", "due_date")


def hypothesis_fingerprint(record: dict[str, Any]) -> str:
    canonical = {field: record.get(field) for field in FINGERPRINT_FIELDS}
    payload = json.dumps(
        canonical,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def apply_matches(dataset: dict[str, Any], overlay: dict[str, Any]) -> tuple[dict[str, Any], int]:
    if overlay.get("schema_version") != "standup_match_overlay_v1":
        raise ValueError("unsupported match overlay schema_version")
    samples = dataset.get("standup")
    meetings = overlay.get("meetings")
    if not isinstance(samples, list):
        raise ValueError("dataset standup must be an array")
    if not isinstance(meetings, list) or not meetings:
        raise ValueError("match overlay meetings must be a non-empty array")
    by_meeting = {
        (sample.get("annotation_source") or {}).get("meeting_id"): sample
        for sample in samples
        if isinstance(sample, dict)
    }
    seen_meetings: set[str] = set()
    applied = 0
    for meeting in meetings:
        if not isinstance(meeting, dict):
            raise ValueError("match overlay meeting must be an object")
        meeting_id = meeting.get("meeting_id")
        if not isinstance(meeting_id, str) or not meeting_id:
            raise ValueError("match overlay meeting_id must be non-empty")
        if meeting_id in seen_meetings:
            raise ValueError(f"duplicate match overlay meeting_id: {meeting_id}")
        seen_meetings.add(meeting_id)
        sample = by_meeting.get(meeting_id)
        if not isinstance(sample, dict):
            raise ValueError(f"match meeting ID is absent from dataset: {meeting_id}")
        hypotheses = sample.get("hypothesis_records")
        references = sample.get("reference_records")
        links = meeting.get("links")
        if not isinstance(hypotheses, list) or not isinstance(references, list):
            raise ValueError(f"{meeting_id} requires hypothesis and reference arrays")
        if not isinstance(links, list):
            raise ValueError(f"{meeting_id} links must be an array")
        reference_by_id = {
            reference.get("id"): reference
            for reference in references
            if isinstance(reference, dict) and reference.get("id")
        }
        for hypothesis in hypotheses:
            if isinstance(hypothesis, dict):
                hypothesis["match_id"] = None
        seen_hypotheses: set[int] = set()
        for link in links:
            if not isinstance(link, dict):
                raise ValueError(f"{meeting_id} link must be an object")
            index = link.get("hypothesis_index")
            expected_kind = link.get("hypothesis_kind")
            expected_fingerprint = link.get("hypothesis_fingerprint")
            reference_id = link.get("reference_id")
            if not isinstance(index, int) or isinstance(index, bool) or not 0 <= index < len(hypotheses):
                raise ValueError(f"{meeting_id} has invalid hypothesis_index")
            if index in seen_hypotheses:
                raise ValueError(f"{meeting_id} hypothesis {index} is linked more than once")
            seen_hypotheses.add(index)
            hypothesis = hypotheses[index]
            reference = reference_by_id.get(reference_id)
            if not isinstance(hypothesis, dict) or not isinstance(reference, dict):
                raise ValueError(f"{meeting_id} link references a missing record")
            if hypothesis.get("kind") != expected_kind or reference.get("kind") != expected_kind:
                raise ValueError(f"{meeting_id} link kind mismatch at hypothesis {index}")
            actual_fingerprint = hypothesis_fingerprint(hypothesis)
            if actual_fingerprint != expected_fingerprint:
                raise ValueError(f"{meeting_id} stale hypothesis fingerprint at index {index}")
            hypothesis["match_id"] = reference_id
            applied += 1
    dataset["match_overlay_schema_version"] = overlay["schema_version"]
    return dataset, applied


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--matches", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--overwrite", action="store_true")
    args = parser.parse_args()
    if args.output.resolve() in {args.dataset.resolve(), args.matches.resolve()}:
        parser.error("output must differ from dataset and match inputs")
    try:
        ensure_output_available(args.output, args.overwrite)
    except ValueError as error:
        parser.error(str(error))
    dataset = load_object(args.dataset)
    overlay = load_object(args.matches)
    linked, count = apply_matches(dataset, overlay)
    atomic_private_write(args.output, linked)
    print(f"Applied {count} reviewed standup match(es).")


if __name__ == "__main__":
    main()
