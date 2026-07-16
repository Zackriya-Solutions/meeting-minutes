#!/usr/bin/env python3

from apply_standup_matches import apply_matches, hypothesis_fingerprint

hypothesis = {
    "kind": "decision",
    "text": "Use the reviewed path",
    "participant": None,
    "owner": None,
    "due_date": None,
    "match_id": "stale-link",
}
dataset = {
    "standup": [{
        "annotation_source": {"meeting_id": "meeting-1"},
        "hypothesis_records": [hypothesis, {"kind": "risk", "text": "unlinked"}],
        "reference_records": [{"id": "gold-1", "kind": "decision"}],
    }]
}
overlay = {
    "schema_version": "standup_match_overlay_v1",
    "meetings": [{
        "meeting_id": "meeting-1",
        "links": [{
            "hypothesis_index": 0,
            "hypothesis_kind": "decision",
            "hypothesis_fingerprint": hypothesis_fingerprint(hypothesis),
            "reference_id": "gold-1",
        }],
    }],
}

result, count = apply_matches(dataset, overlay)
assert count == 1
assert result["standup"][0]["hypothesis_records"][0]["match_id"] == "gold-1"
assert result["standup"][0]["hypothesis_records"][1]["match_id"] is None

stale = {
    **overlay,
    "meetings": [{
        **overlay["meetings"][0],
        "links": [{**overlay["meetings"][0]["links"][0], "hypothesis_fingerprint": "0" * 64}],
    }],
}
try:
    apply_matches(dataset, stale)
    raise AssertionError("stale model output must invalidate reviewed links")
except ValueError as error:
    assert "stale hypothesis fingerprint" in str(error)

print("ok - private standup match overlay")
