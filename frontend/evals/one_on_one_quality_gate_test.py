#!/usr/bin/env python3

from one_on_one_quality_gate import failures, metrics


def sample(index: int, meeting_type: str, split: str, output: bool = True):
    reference = {
        "id": f"ref-{index}",
        "kind": "commitment",
        "payload": {"task": "Do the thing", "owner": "Alex"},
    }
    hypothesis = {
        "kind": "commitment",
        "match_id": reference["id"],
        "payload": {
            "task": "Do the thing",
            "owner": "Alex",
            "evidence": [{"timestamp": "[00:01]", "quote": "Do the thing"}],
        },
    }
    return {
        "id": f"meeting-{index}",
        "meeting_type": meeting_type,
        "pair_id": f"pair-{index}",
        "split": split,
        "valid_timestamps": ["[00:01]"],
        "reference_records": [reference] if output else [],
        "hypothesis_records": [hypothesis] if output else [],
        "run": {"success": meeting_type == "one_on_one"},
    }


dataset = {
    "schema_version": "one_on_one_corpus_v1",
    "samples": [
        *[sample(i, "one_on_one", "dev" if i < 2 else "test" if i < 5 else "train") for i in range(8)],
        sample(20, "pair_programming", "test", False),
        sample(21, "technical_deep_dive", "test", False),
        sample(22, "interview", "test", False),
        sample(23, "project_status", "test", False),
    ],
}

result = metrics(dataset)
assert failures(result) == []
assert result["evidence_validity"] == 1.0
assert result["owner_precision_when_shown"] == 1.0

dataset["samples"][0]["hypothesis_records"][0]["payload"]["owner"] = "Wrong owner"
dataset["samples"][8]["hypothesis_records"] = [{"kind": "commitment", "payload": {}}]
dataset["samples"][0]["hypothesis_records"].extend(
    {"kind": "feedback", "match_id": f"feedback-{index}", "payload": {}}
    for index in range(100)
)
broken = metrics(dataset)
assert broken["owner_precision_when_shown"] < 0.95
assert broken["contrast_false_positive_rate"] > 0
assert broken["unsupported_commitment_decision_rate"] == 1 / 9
assert failures(broken)

dataset["samples"][0]["hypothesis_records"][0]["payload"]["analysis"] = "риск увольнения"
assert metrics(dataset)["forbidden_people_inference_count"] == 1

print("ok - one-on-one quality gate")
