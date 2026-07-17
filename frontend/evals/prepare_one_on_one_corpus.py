#!/usr/bin/env python3
"""Export a private One-on-One Memory annotation skeleton from Memento.

The exporter is deliberately read-only and never guesses a pair, role, date, meeting type, or
split. Transcript text is written with mode 0600 and must stay outside version control.
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import tempfile
from pathlib import Path
from typing import Any


def table_exists(db: sqlite3.Connection, table: str) -> bool:
    return db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)", (table,)
    ).fetchone()[0] == 1


def column_exists(db: sqlite3.Connection, table: str, column: str) -> bool:
    return any(row[1] == column for row in db.execute(f"PRAGMA table_info({table})"))


def atomic_private_write(path: Path, value: dict[str, Any], overwrite: bool) -> None:
    if path.exists() and not overwrite:
        raise ValueError(f"output already exists: {path}; pass --overwrite to replace it")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(value, output, ensure_ascii=False, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        temporary.replace(path)
        os.chmod(path, 0o600)
    finally:
        temporary.unlink(missing_ok=True)


def timestamp(seconds: float | None) -> str:
    seconds = max(0, int(seconds or 0))
    return f"[{seconds // 60:02d}:{seconds % 60:02d}]"


def flatten_report(report: dict[str, Any]) -> list[dict[str, Any]]:
    mapping = {
        "check_in": "check_in",
        "previous_follow_ups": "previous_follow_up",
        "progress": "progress",
        "challenges_and_support": "challenge_support",
        "feedback": "feedback",
        "growth": "growth",
        "decisions": "decision",
        "commitments": "commitment",
        "open_topics": "open_topic",
    }
    rows: list[dict[str, Any]] = []
    for field, kind in mapping.items():
        for payload in report.get(field) or []:
            rows.append({"kind": kind, "payload": payload, "match_id": None})
    return rows


def result_for(db: sqlite3.Connection, meeting_id: str) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if not table_exists(db, "summary_processes"):
        return [], {}
    row = db.execute(
        "SELECT result,processing_time,chunk_count,status FROM summary_processes WHERE meeting_id=?",
        (meeting_id,),
    ).fetchone()
    if not row or not row[0]:
        return [], {"success": False, "status": row[3] if row else "missing"}
    try:
        value = json.loads(row[0])
    except json.JSONDecodeError:
        return [], {"success": False, "status": "invalid_json"}
    report = value.get("one_on_one_v1")
    source = ((value.get("summary_generation") or {}).get("source") or {})
    return (
        flatten_report(report) if isinstance(report, dict) else [],
        {
            "success": isinstance(report, dict),
            "status": row[3],
            "latency_ms": round(float(row[1] or 0) * 1000),
            "chunk_count": row[2],
            "provider": source.get("model_provider", "unknown"),
            "model": source.get("model_name", "unknown"),
            "template_fingerprint": source.get("template_fingerprint", "UNASSIGNED"),
        },
    )


def accepted_references(db: sqlite3.Connection, meeting_id: str) -> list[dict[str, Any]]:
    if not table_exists(db, "one_on_one_records"):
        return []
    rows = db.execute(
        "SELECT id,kind,payload,reviewed_payload FROM one_on_one_records "
        "WHERE meeting_id=? AND review_status='accepted' ORDER BY id",
        (meeting_id,),
    ).fetchall()
    return [
        {
            "id": f"accepted-{row[0]}",
            "kind": row[1],
            "payload": json.loads(row[3] or row[2]),
        }
        for row in rows
    ]


def collection_ids(db: sqlite3.Connection, collection_name: str) -> list[str]:
    if not table_exists(db, "collections") or not table_exists(db, "meeting_collections"):
        raise ValueError("database has no collections schema")
    rows = db.execute(
        "SELECT mc.meeting_id FROM meeting_collections mc JOIN collections c ON c.id=mc.collection_id "
        "WHERE lower(c.name)=lower(?) ORDER BY mc.meeting_id",
        (collection_name,),
    ).fetchall()
    if not rows:
        raise ValueError(f"collection not found or empty: {collection_name}")
    return [row[0] for row in rows]


def export(db: sqlite3.Connection, meeting_ids: list[str]) -> dict[str, Any]:
    occurred_confirmed = column_exists(db, "meetings", "occurred_at_confirmed")
    samples: list[dict[str, Any]] = []
    for meeting_id in meeting_ids:
        meeting = db.execute(
            "SELECT id,title,occurred_at FROM meetings WHERE id=?", (meeting_id,)
        ).fetchone()
        if not meeting:
            raise ValueError(f"meeting not found: {meeting_id}")
        segments = db.execute(
            "SELECT transcript,audio_start_time,speaker_id FROM transcripts WHERE meeting_id=? "
            "AND trim(transcript)!='' ORDER BY COALESCE(audio_start_time,0),timestamp,id",
            (meeting_id,),
        ).fetchall()
        if not segments:
            raise ValueError(f"meeting has no transcript: {meeting_id}")
        confirmed = False
        if occurred_confirmed:
            confirmed = bool(db.execute(
                "SELECT occurred_at_confirmed FROM meetings WHERE id=?", (meeting_id,)
            ).fetchone()[0])
        hypothesis, run = result_for(db, meeting_id)
        samples.append(
            {
                "id": meeting_id,
                "title": meeting[1],
                "occurred_at": meeting[2],
                "occurred_at_confirmed": confirmed,
                "pair_id": "UNASSIGNED",
                "split": "unassigned",
                "meeting_type": "UNASSIGNED",
                "recording_scope": "unknown",
                "roles_confirmed": False,
                "speakers_confirmed": all(row[2] is not None for row in segments),
                "transcript": "\n".join(f"{timestamp(row[1])} {row[0].strip()}" for row in segments),
                "valid_timestamps": [timestamp(row[1]) for row in segments],
                "hypothesis_records": hypothesis,
                "reference_records": accepted_references(db, meeting_id),
                "run": run,
                "review_state": "manual_review_required",
                "review_notes": {"losses": [], "hallucinations": [], "attribution_errors": []},
            }
        )
    return {
        "schema_version": "one_on_one_corpus_v1",
        "annotation_policy": "manual transcript review; pair/date/roles/type are never inferred",
        "meeting_type_options": [
            "one_on_one", "pair_programming", "technical_deep_dive", "interview", "project_status", "uncertain"
        ],
        "samples": samples,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--collection", default="1to1")
    parser.add_argument("--meeting-id", action="append", default=[])
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--overwrite", action="store_true")
    args = parser.parse_args()
    if not args.db.is_file():
        parser.error(f"database not found: {args.db}")
    try:
        db = sqlite3.connect(f"file:{args.db.resolve()}?mode=ro", uri=True)
        db.row_factory = sqlite3.Row
        meeting_ids = [value.strip() for value in args.meeting_id if value.strip()]
        if not meeting_ids:
            meeting_ids = collection_ids(db, args.collection)
            if args.limit:
                meeting_ids = meeting_ids[: args.limit]
        dataset = export(db, meeting_ids)
        atomic_private_write(args.output, dataset, args.overwrite)
    except (sqlite3.Error, ValueError, OSError) as error:
        parser.error(str(error))
    print(json.dumps({"output": str(args.output), "samples": len(dataset["samples"])}, ensure_ascii=False))


if __name__ == "__main__":
    main()
