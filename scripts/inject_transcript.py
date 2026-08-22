#!/usr/bin/env python3
"""
Meeting Transcript Database Injector

Injects CSV-based transcript data into the Meetily SQLite database,
creating meeting entries identical to those from normal recordings.

Usage:
    python inject_transcript.py --csv transcript.csv --title "Test Meeting"
    python inject_transcript.py --csv transcript.csv --db /path/to/db.sqlite

CSV Format (minimal - text column only):
    text
    "Hello everyone, let's start the meeting."
    "First item on the agenda is the Q1 roadmap."
"""

import argparse
import csv
import os
import platform
import sqlite3
import sys
import uuid
from datetime import datetime, timedelta
from pathlib import Path


def get_default_db_path() -> Path:
    """Get the default database path based on the platform."""
    system = platform.system()

    if system == "Darwin":  # macOS
        base_path = Path.home() / "Library" / "Application Support" / "Meetily"
    elif system == "Windows":
        appdata = os.environ.get("APPDATA", "")
        if appdata:
            base_path = Path(appdata) / "Meetily"
        else:
            base_path = Path.home() / "AppData" / "Roaming" / "Meetily"
    else:  # Linux and others
        base_path = Path.home() / ".config" / "Meetily"

    return base_path / "meeting_minutes.sqlite"


def estimate_duration(text: str) -> float:
    """
    Estimate speech duration from text length.

    Assumes ~150 words per minute speech rate, which equals ~0.4 seconds per word.
    """
    word_count = len(text.split())
    # ~0.4 seconds per word (150 words/minute)
    duration = word_count * 0.4
    # Minimum duration of 0.5 seconds for very short segments
    return max(duration, 0.5)


def read_csv(csv_path: str) -> list[dict]:
    """Read transcript segments from CSV file."""
    segments = []

    with open(csv_path, 'r', encoding='utf-8') as f:
        reader = csv.DictReader(f)

        # Check for required 'text' column
        if 'text' not in reader.fieldnames:
            raise ValueError("CSV must have a 'text' column")

        for row in reader:
            text = row.get('text', '').strip()
            if text:
                segments.append({'text': text})

    if not segments:
        raise ValueError("CSV file contains no transcript segments")

    return segments


def process_segments(segments: list[dict], start_time: datetime) -> list[dict]:
    """
    Process segments to add IDs, timestamps, and audio timing.

    Args:
        segments: List of dicts with 'text' key
        start_time: Meeting start time for timestamp generation

    Returns:
        List of fully processed segment dicts ready for database insertion
    """
    processed = []
    current_audio_time = 0.0
    current_timestamp = start_time

    for i, segment in enumerate(segments):
        text = segment['text']
        duration = estimate_duration(text)

        processed.append({
            'id': f"seg-{uuid.uuid4()}",
            'text': text,
            'timestamp': current_timestamp.isoformat(),
            'audio_start_time': current_audio_time,
            'audio_end_time': current_audio_time + duration,
            'duration': duration,
        })

        # Advance timing for next segment
        current_audio_time += duration
        current_timestamp += timedelta(seconds=duration)

    return processed


def inject_meeting(
    db_path: str,
    title: str,
    segments: list[dict],
    created_at: datetime,
    folder_path: str | None = None
) -> str:
    """
    Inject a meeting with transcripts into the database.

    Args:
        db_path: Path to SQLite database file
        title: Meeting title
        segments: Processed transcript segments
        created_at: Meeting creation timestamp
        folder_path: Optional path to audio folder

    Returns:
        The generated meeting_id
    """
    meeting_id = f"meeting-{uuid.uuid4()}"
    now = created_at.isoformat()

    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    try:
        # Begin transaction
        cursor.execute("BEGIN TRANSACTION")

        # Insert meeting
        cursor.execute("""
            INSERT INTO meetings (id, title, created_at, updated_at, folder_path)
            VALUES (?, ?, ?, ?, ?)
        """, (meeting_id, title, now, now, folder_path))

        # Insert transcript segments
        for seg in segments:
            cursor.execute("""
                INSERT INTO transcripts (
                    id, meeting_id, transcript, timestamp,
                    audio_start_time, audio_end_time, duration
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """, (
                seg['id'],
                meeting_id,
                seg['text'],
                seg['timestamp'],
                seg['audio_start_time'],
                seg['audio_end_time'],
                seg['duration'],
            ))

        # Commit transaction
        conn.commit()

    except Exception as e:
        conn.rollback()
        raise RuntimeError(f"Database insertion failed: {e}")
    finally:
        conn.close()

    return meeting_id


def verify_injection(db_path: str, meeting_id: str) -> dict:
    """
    Verify the meeting was injected correctly.

    Returns:
        Dict with meeting info and transcript count
    """
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    try:
        # Get meeting info
        cursor.execute("""
            SELECT id, title, created_at, folder_path
            FROM meetings WHERE id = ?
        """, (meeting_id,))
        meeting = cursor.fetchone()

        if not meeting:
            raise RuntimeError(f"Meeting {meeting_id} not found after insertion")

        # Count transcripts
        cursor.execute("""
            SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?
        """, (meeting_id,))
        transcript_count = cursor.fetchone()[0]

        # Get total duration
        cursor.execute("""
            SELECT MAX(audio_end_time) FROM transcripts WHERE meeting_id = ?
        """, (meeting_id,))
        total_duration = cursor.fetchone()[0] or 0.0

        return {
            'meeting_id': meeting[0],
            'title': meeting[1],
            'created_at': meeting[2],
            'folder_path': meeting[3],
            'transcript_count': transcript_count,
            'total_duration_seconds': total_duration,
        }

    finally:
        conn.close()


def main():
    parser = argparse.ArgumentParser(
        description="Inject CSV transcript data into Meetily database",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
CSV Format (minimal - just 'text' column required):
  text
  "Hello everyone, let's start the meeting."
  "First item on the agenda is the Q1 roadmap."

Example usage:
  python inject_transcript.py --csv transcript.csv --title "Team Standup"
  python inject_transcript.py --csv data.csv --db ~/custom/path.sqlite
        """
    )

    parser.add_argument(
        '--csv', '-c',
        required=True,
        help='Path to CSV file with transcript segments'
    )

    parser.add_argument(
        '--db', '-d',
        default=None,
        help='Database path (auto-detects platform default if not specified)'
    )

    parser.add_argument(
        '--title', '-t',
        default=None,
        help='Meeting title (defaults to "Injected Meeting - <timestamp>")'
    )

    parser.add_argument(
        '--created-at',
        default=None,
        help='Meeting creation timestamp in ISO format (defaults to now)'
    )

    parser.add_argument(
        '--folder-path', '-f',
        default=None,
        help='Optional path to audio folder'
    )

    args = parser.parse_args()

    # Resolve database path
    if args.db:
        db_path = Path(args.db)
    else:
        db_path = get_default_db_path()

    if not db_path.exists():
        print(f"Error: Database not found at {db_path}", file=sys.stderr)
        print("Make sure Meetily has been run at least once to create the database.", file=sys.stderr)
        sys.exit(1)

    # Resolve CSV path
    csv_path = Path(args.csv)
    if not csv_path.exists():
        print(f"Error: CSV file not found at {csv_path}", file=sys.stderr)
        sys.exit(1)

    # Parse creation timestamp
    if args.created_at:
        try:
            created_at = datetime.fromisoformat(args.created_at.replace('Z', '+00:00'))
        except ValueError:
            print(f"Error: Invalid timestamp format: {args.created_at}", file=sys.stderr)
            print("Use ISO format, e.g.: 2025-12-05T10:00:00Z", file=sys.stderr)
            sys.exit(1)
    else:
        created_at = datetime.now()

    # Generate title if not provided
    title = args.title or f"Injected Meeting - {created_at.strftime('%Y-%m-%d %H:%M')}"

    print(f"Reading CSV: {csv_path}")
    try:
        segments = read_csv(str(csv_path))
    except Exception as e:
        print(f"Error reading CSV: {e}", file=sys.stderr)
        sys.exit(1)

    print(f"Processing {len(segments)} transcript segments...")
    processed_segments = process_segments(segments, created_at)

    print(f"Injecting into database: {db_path}")
    try:
        meeting_id = inject_meeting(
            str(db_path),
            title,
            processed_segments,
            created_at,
            args.folder_path
        )
    except Exception as e:
        print(f"Error injecting meeting: {e}", file=sys.stderr)
        sys.exit(1)

    # Verify and print summary
    print("\n" + "=" * 50)
    print("SUCCESS: Meeting injected")
    print("=" * 50)

    try:
        info = verify_injection(str(db_path), meeting_id)
        print(f"  Meeting ID:      {info['meeting_id']}")
        print(f"  Title:           {info['title']}")
        print(f"  Created At:      {info['created_at']}")
        print(f"  Segments:        {info['transcript_count']}")
        print(f"  Total Duration:  {info['total_duration_seconds']:.1f} seconds")
        if info['folder_path']:
            print(f"  Folder Path:     {info['folder_path']}")
    except Exception as e:
        print(f"Warning: Verification failed: {e}", file=sys.stderr)

    print("\nThe meeting should now appear in the Meetily sidebar.")


if __name__ == "__main__":
    main()
