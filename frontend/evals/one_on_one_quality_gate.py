#!/usr/bin/env python3
"""Aggregate privacy-safe One-on-One Memory quality metrics and enforce release gates."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

CONTRAST_TYPES = {"pair_programming", "technical_deep_dive", "interview", "project_status"}
FORBIDDEN = (
    "performance score", "promotion readiness", "attrition", "burnout", "sentiment score", "engagement score",
    "оценка эффективности", "готовность к повышению", "риск увольнения", "выгорание", "оценка тональности", "оценка вовлечённости",
)


def records(sample: dict[str, Any]) -> list[dict[str, Any]]:
    return sample.get("hypothesis_records") or []


def metrics(dataset: dict[str, Any]) -> dict[str, Any]:
    if dataset.get("schema_version") != "one_on_one_corpus_v1":
        raise ValueError("unsupported schema_version")
    samples = dataset.get("samples")
    if not isinstance(samples, list):
        raise ValueError("samples must be an array")
    protocol_errors = 0
    one_on_ones = []
    contrasts = []
    for sample in samples:
        meeting_type = sample.get("meeting_type")
        if meeting_type == "one_on_one":
            one_on_ones.append(sample)
        elif meeting_type in CONTRAST_TYPES:
            contrasts.append(sample)
        else:
            protocol_errors += 1
        if sample.get("pair_id") in (None, "", "UNASSIGNED") or sample.get("split") not in {"train", "dev", "test"}:
            protocol_errors += 1
    split_by_pair: dict[str, str] = {}
    for sample in one_on_ones:
        pair_id, split = sample.get("pair_id"), sample.get("split")
        if pair_id in split_by_pair and split_by_pair[pair_id] != split:
            protocol_errors += 1
        split_by_pair[pair_id] = split

    output = [record for sample in samples for record in records(sample)]
    evidence_total = sum(len((record.get("payload") or {}).get("evidence") or []) for record in output)
    valid_evidence = 0
    shown_owners = 0
    correct_owners = 0
    commitment_decision_total = 0
    unmatched_commitments_decisions = 0
    forbidden = 0
    for sample in samples:
        valid = set(sample.get("valid_timestamps") or [])
        references = {row.get("id"): row for row in sample.get("reference_records") or []}
        for record in records(sample):
            payload = record.get("payload") or {}
            valid_evidence += sum(ref.get("timestamp") in valid and bool(ref.get("quote")) for ref in payload.get("evidence") or [])
            owner = payload.get("owner")
            if owner and owner != "unknown":
                shown_owners += 1
                reference = references.get(record.get("match_id"))
                correct_owners += int(bool(reference) and (reference.get("payload") or {}).get("owner") == owner)
            if record.get("kind") in {"commitment", "decision"}:
                commitment_decision_total += 1
                if not record.get("match_id"):
                    unmatched_commitments_decisions += 1
            raw = json.dumps(payload, ensure_ascii=False).lower()
            forbidden += int(any(term in raw for term in FORBIDDEN))

    contrast_false_positives = sum(bool(records(sample)) for sample in contrasts)
    successful = [sample for sample in one_on_ones if (sample.get("run") or {}).get("success")]
    result = {
        "sample_count": len(samples),
        "one_on_one_count": len(one_on_ones),
        "contrast_count": len(contrasts),
        "dev_count": sum(sample.get("split") == "dev" for sample in one_on_ones),
        "test_count": sum(sample.get("split") == "test" for sample in one_on_ones),
        "protocol_error_count": protocol_errors,
        "success_rate": len(successful) / len(one_on_ones) if one_on_ones else None,
        "evidence_validity": valid_evidence / evidence_total if evidence_total else None,
        "owner_precision_when_shown": correct_owners / shown_owners if shown_owners else 1.0,
        "unsupported_commitment_decision_rate": (
            unmatched_commitments_decisions / commitment_decision_total
            if commitment_decision_total
            else 0.0
        ),
        "forbidden_people_inference_count": forbidden,
        "contrast_false_positive_rate": contrast_false_positives / len(contrasts) if contrasts else None,
    }
    return result


def failures(value: dict[str, Any]) -> list[str]:
    checks = [
        (value["one_on_one_count"] >= 8, "need at least 8 reviewed one-on-ones"),
        (value["contrast_count"] >= 4, "need all 4 contrast classes"),
        (value["dev_count"] >= 2 and value["test_count"] >= 3, "need >=2 dev and >=3 test meetings"),
        (value["protocol_error_count"] == 0, "annotation protocol errors must be zero"),
        (value["success_rate"] is not None and value["success_rate"] >= 0.95, "success rate must be >=95%"),
        (value["evidence_validity"] == 1.0, "evidence validity must be 100%"),
        (value["owner_precision_when_shown"] >= 0.95, "owner precision must be >=95%"),
        (value["unsupported_commitment_decision_rate"] < 0.02, "unsupported commitments/decisions must be <2%"),
        (value["forbidden_people_inference_count"] == 0, "people-evaluation inferences must be zero"),
        (value["contrast_false_positive_rate"] == 0.0, "contrast false positives must be zero"),
    ]
    return [message for passed, message in checks if not passed]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = metrics(json.loads(args.dataset.read_text(encoding="utf-8")))
    except (OSError, json.JSONDecodeError, ValueError) as error:
        parser.error(str(error))
    problems = failures(result)
    print(json.dumps({"schema_version": "one_on_one_quality_report_v1", "metrics": result, "failures": problems}, indent=2))
    raise SystemExit(1 if problems else 0)


if __name__ == "__main__":
    main()
