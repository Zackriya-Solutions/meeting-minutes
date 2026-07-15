#!/usr/bin/env python3
"""Build a private Standup V2 annotation/evaluation skeleton from a Memento database.

The output intentionally contains meeting text and must stay under evals/private/ or another
private location. Aggregate quality-gate reports never copy that content.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sqlite3
from pathlib import Path
from typing import Any


MEETING_TYPES = (
    "pure_status",
    "status_plus_deep_dive",
    "planning_sync",
    "one_to_one",
    "general_meeting",
    "uncertain",
)


def table_exists(db: sqlite3.Connection, table: str) -> bool:
    return db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)", (table,)
    ).fetchone()[0] == 1


def column_exists(db: sqlite3.Connection, table: str, column: str) -> bool:
    return any(row[1] == column for row in db.execute(f"PRAGMA table_info({table})"))


def timestamp(seconds: float | None) -> str | None:
    if seconds is None or seconds < 0:
        return None
    rounded = int(seconds)
    return f"[{rounded // 60:02d}:{rounded % 60:02d}]"


def primary_text(kind: str, payload: dict[str, Any]) -> str:
    field = {
        "decision": "decision",
        "action": "task",
        "risk": "blocker_or_risk",
        "deep_dive": "topic",
    }.get(kind, "text")
    return str(payload.get(field) or "")


def record_shape(kind: str, payload: dict[str, Any], match_id: str | None = None) -> dict[str, Any]:
    return {
        "kind": kind,
        "match_id": match_id,
        "text": primary_text(kind, payload),
        "owner": payload.get("owner"),
        "due_date": payload.get("due_date"),
        "participant": payload.get("participant"),
        "category": payload.get("category"),
        "evidence": payload.get("evidence") or [],
    }


def flatten_standup(report: dict[str, Any]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for item in report.get("overview") or []:
        rows.append(record_shape("overview", item))
    for update in report.get("participant_updates") or []:
        for category in ("completed_or_recent", "next", "blockers"):
            for item in update.get(category) or []:
                rows.append(
                    record_shape(
                        "participant_update",
                        {**item, "participant": update.get("participant"), "category": category},
                    )
                )
    for source, kind in (
        ("decisions", "decision"),
        ("action_items", "action"),
        ("risks_and_blockers", "risk"),
        ("deep_dives", "deep_dive"),
        ("unattributed_facts", "unattributed_fact"),
    ):
        for item in report.get(source) or []:
            rows.append(record_shape(kind, item))
    return rows


def generation_identity(result: dict[str, Any]) -> tuple[str, str, str]:
    """Read non-secret run identity from the persisted summary cache source."""
    source = ((result.get("summary_generation") or {}).get("source") or {})
    return (
        str(source.get("model_provider") or "unknown"),
        str(source.get("model_name") or "unknown"),
        str(source.get("template_fingerprint") or "UNASSIGNED"),
    )


def reviewed_records(
    db: sqlite3.Connection, meeting_id: str
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]] | None:
    if not table_exists(db, "standup_records"):
        return None
    stored = db.execute(
        "SELECT id, kind, payload, reviewed_payload, review_status "
        "FROM standup_records WHERE meeting_id=? ORDER BY id",
        (meeting_id,),
    ).fetchall()
    if not stored:
        return None
    references: list[dict[str, Any]] = []
    hypotheses: list[dict[str, Any]] = []
    for row in stored:
        payload = json.loads(row[3] or row[2])
        reference_id = f"review-{row[0]}" if row[4] == "accepted" else None
        hypothesis = record_shape(row[1], payload, reference_id)
        hypothesis["review_status"] = row[4]
        hypotheses.append(hypothesis)
        if reference_id:
            references.append(
                {
                    "id": reference_id,
                    "kind": row[1],
                    "text": primary_text(row[1], payload),
                    "owner": payload.get("owner"),
                    "due_date": payload.get("due_date"),
                }
            )
    return references, hypotheses


def candidate_score(title: str, occurred_at: str, transcript: str, duration_seconds: float) -> tuple[int, list[str]]:
    title_lower = title.lower()
    text_lower = transcript.lower()
    score = 0
    reasons: list[str] = []
    if "standup" in title_lower or "стендап" in title_lower:
        score += 20
        reasons.append("title contains standup")
    content_markers = ["стендап", "блокер", "вчера", "сегодня", "что сделал", "что буду", "дальше"]
    hits = sum(marker in text_lower for marker in content_markers)
    if hits:
        score += min(hits, 6)
        reasons.append(f"{hits} status-round content markers")
    time_match = re.search(r"T(\d{2}):(\d{2})", occurred_at or "")
    if time_match and (10, 30) <= (int(time_match.group(1)), int(time_match.group(2))) <= (12, 0):
        score += 3
        reasons.append("10:30-12:00 time window")
    if 5 * 60 <= duration_seconds <= 45 * 60:
        score += 2
        reasons.append("standup-like duration")
    if any(marker in title_lower for marker in ("one-to-one", "planning", "ретро")):
        score -= 4
        reasons.append("title suggests another meeting type")
    return score, reasons


def split_for_series(series_id: str) -> str:
    bucket = int(hashlib.sha256(series_id.encode("utf-8")).hexdigest()[:8], 16) % 10
    return "train" if bucket < 7 else "dev" if bucket < 9 else "test"


def series_for_meeting(db: sqlite3.Connection, meeting_id: str) -> tuple[str, str, list[str]]:
    if not table_exists(db, "collections") or not table_exists(db, "meeting_collections"):
        return "UNASSIGNED", "unassigned", []
    rows = db.execute(
        "SELECT c.id, c.name FROM collections c "
        "JOIN meeting_collections mc ON mc.collection_id=c.id "
        "WHERE mc.meeting_id=? AND c.kind='series' ORDER BY c.id",
        (meeting_id,),
    ).fetchall()
    if len(rows) != 1:
        return "UNASSIGNED", "unassigned", [row[1] for row in rows]
    series_id = f"series-{rows[0][0]}"
    return series_id, split_for_series(series_id), [rows[0][1]]


def select_samples(
    candidates: list[tuple[int, dict[str, Any]]],
    explicit_meeting_ids: list[str],
    limit: int,
) -> list[dict[str, Any]]:
    """Freeze an explicit selection or return the current ranked candidate slice."""
    if not explicit_meeting_ids:
        return [item[1] for item in candidates[: limit or None]]
    normalized = [value.strip() for value in explicit_meeting_ids if value.strip()]
    if len(normalized) != len(set(normalized)):
        raise ValueError("Explicit meeting IDs must be unique")
    by_meeting_id = {
        item[1]["annotation_source"]["meeting_id"]: item[1] for item in candidates
    }
    missing = [meeting_id for meeting_id in normalized if meeting_id not in by_meeting_id]
    if missing:
        raise ValueError(f"Meeting IDs not found or without transcripts: {', '.join(missing)}")
    return [by_meeting_id[meeting_id] for meeting_id in normalized]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True, type=Path, help="Path to meeting_minutes.sqlite")
    parser.add_argument("--output", type=Path, default=Path("evals/private/standup-corpus.json"))
    parser.add_argument("--limit", type=int, default=15, help="Top candidates to export; 0 exports all")
    parser.add_argument(
        "--meeting-id",
        action="append",
        default=[],
        help="Exact meeting ID to export; repeat to freeze a reviewed set in this order",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Replace an existing output file (intended for disposable drafts only)",
    )
    args = parser.parse_args()

    if args.output.exists() and not args.overwrite:
        parser.error(
            f"Output already exists: {args.output}. Refusing to overwrite private labels; "
            "choose another path or use --overwrite for a disposable draft."
        )

    db = sqlite3.connect(f"file:{args.db.resolve()}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    occurred = "COALESCE(m.occurred_at, m.created_at)" if column_exists(db, "meetings", "occurred_at") else "m.created_at"
    meetings = db.execute(
        f"SELECT m.id, m.title, {occurred} AS occurred_at, sp.result, sp.status, sp.processing_time "
        "FROM meetings m LEFT JOIN summary_processes sp ON sp.meeting_id=m.id "
        "WHERE EXISTS(SELECT 1 FROM transcripts t WHERE t.meeting_id=m.id AND trim(t.transcript)!='')"
    ).fetchall()

    candidates: list[tuple[int, dict[str, Any]]] = []
    for meeting in meetings:
        segments = db.execute(
            "SELECT transcript, audio_start_time, audio_end_time, speaker, speaker_id "
            "FROM transcripts WHERE meeting_id=? ORDER BY audio_start_time, timestamp, id",
            (meeting["id"],),
        ).fetchall()
        transcript_rows = []
        valid_timestamps = []
        for segment in segments:
            mark = timestamp(segment["audio_start_time"])
            if mark:
                valid_timestamps.append(mark)
            transcript_rows.append(
                {
                    "timestamp": mark,
                    "text": segment["transcript"],
                    "speaker": segment["speaker"],
                    "speaker_id": segment["speaker_id"],
                }
            )
        duration = max((row["audio_end_time"] or 0 for row in segments), default=0)
        transcript_text = "\n".join(row["text"] for row in transcript_rows)
        score, reasons = candidate_score(
            meeting["title"], meeting["occurred_at"] or "", transcript_text, duration
        )
        series_id, split, series_names = series_for_meeting(db, meeting["id"])

        result: dict[str, Any] = {}
        try:
            result = json.loads(meeting["result"] or "{}")
        except json.JSONDecodeError:
            pass
        provider, model, prompt_version = generation_identity(result)
        reviewed = reviewed_records(db, meeting["id"])
        if reviewed:
            references, hypotheses = reviewed
        else:
            references = []
            hypotheses = flatten_standup(result.get("standup_v2") or {})

        sample = {
            "id": f"meeting-{meeting['id']}",
            "series_id": series_id,
            "series_names": series_names,
            "split": split,
            "success": meeting["status"] == "completed" and bool(result.get("standup_v2")),
            "latency_ms": round((meeting["processing_time"] or 0) * 1000),
            "provider": provider,
            "model": model,
            "schema_version": (result.get("standup_v2") or {}).get("schema_version", "UNASSIGNED"),
            "prompt_version": prompt_version,
            "candidate_score": score,
            "candidate_reasons": reasons,
            "meeting_type": "UNASSIGNED",
            "review_state": "needs_reference_completion",
            "valid_timestamps": sorted(set(valid_timestamps)),
            "reference_records": references,
            "hypothesis_records": hypotheses,
            "annotation_source": {
                "meeting_id": meeting["id"],
                "title": meeting["title"],
                "occurred_at": meeting["occurred_at"],
                "duration_seconds": duration,
                "transcript": transcript_rows,
            },
        }
        candidates.append((score, sample))

    candidates.sort(key=lambda item: (-item[0], item[1]["annotation_source"]["occurred_at"] or ""))
    try:
        samples = select_samples(candidates, args.meeting_id, args.limit)
    except ValueError as error:
        parser.error(str(error))
    payload = {
        "dataset_id": f"memento-private-standup-{dt.datetime.now(dt.timezone.utc).date().isoformat()}",
        "schema_version": "standup_eval_v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "meeting_type_options": list(MEETING_TYPES),
        "standup": samples,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_suffix(f"{args.output.suffix}.tmp")
    temporary.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.chmod(0o600)
    temporary.replace(args.output)
    args.output.chmod(0o600)
    assigned = sum(row["series_id"] != "UNASSIGNED" for row in samples)
    generated = sum(row["success"] for row in samples)
    print(f"Exported {len(samples)} candidates to {args.output}")
    print(f"Series assigned: {assigned}/{len(samples)}; Standup V2 generated: {generated}/{len(samples)}")
    print("Private transcript content was written only to the requested output file.")


if __name__ == "__main__":
    main()
