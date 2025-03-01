import aiosqlite
import json
import logging
import uuid
from datetime import datetime, timedelta
from typing import Optional, List, Dict, Any
from contextlib import asynccontextmanager

logger = logging.getLogger(__name__)

class DatabaseManager:
    def __init__(self, db_path: str = "summaries.db"):
        self.db_path = db_path

    async def initialize_db(self):
        """Initialize the database asynchronously with required tables."""
        async with aiosqlite.connect(self.db_path) as conn:
            await conn.executescript("""
                CREATE TABLE IF NOT EXISTS summary_processes (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    result TEXT,
                    error TEXT,
                    start_time TEXT,
                    end_time TEXT,
                    chunk_count INTEGER DEFAULT 0,
                    processing_time REAL DEFAULT 0.0,
                    metadata TEXT
                );

                CREATE TABLE IF NOT EXISTS transcripts (
                    process_id TEXT PRIMARY KEY,
                    meeting_name TEXT,
                    transcript_text TEXT NOT NULL,
                    model TEXT NOT NULL,
                    model_name TEXT NOT NULL,
                    chunk_size INTEGER,
                    overlap INTEGER,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (process_id) REFERENCES summary_processes(id)
                );

                CREATE TABLE IF NOT EXISTS meetings (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    date TEXT NOT NULL,
                    time TEXT,
                    attendees TEXT,
                    tags TEXT,
                    content TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    deleted_at TEXT
                );
            """)
            await conn.commit()
            logger.info("Database initialized.")

    @asynccontextmanager
    async def _get_connection(self):
        """Provide a database connection using a context manager."""
        conn = await aiosqlite.connect(self.db_path)
        try:
            yield conn
        finally:
            await conn.close()

    async def create_meeting(self, title: str, date: str, time: Optional[str] = None,
                             attendees: Optional[List[str]] = None, tags: Optional[List[str]] = None,
                             content: Optional[str] = None) -> str:
        """Create a new meeting record and return the meeting ID."""
        meeting_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                """
                INSERT INTO meetings (id, title, date, time, attendees, tags, content, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    meeting_id,
                    title,
                    date,
                    time,
                    json.dumps(attendees or []),
                    json.dumps(tags or []),
                    content or "",
                    now,
                    now
                )
            )
            await conn.commit()

        logger.info(f"Created meeting {meeting_id}")
        return meeting_id

    async def get_meetings(self) -> List[Dict[str, Any]]:
        """Retrieve all non-deleted meetings."""
        async with self._get_connection() as conn:
            async with conn.execute(
                "SELECT * FROM meetings WHERE deleted_at IS NULL ORDER BY date DESC"
            ) as cursor:
                columns = [col[0] for col in cursor.description]
                return [
                    {
                        col: (
                            json.loads(row[idx])
                            if col in ("attendees", "tags") and row[idx]
                            else row[idx]
                        )
                        for idx, col in enumerate(columns)
                    }
                    for row in await cursor.fetchall()
                ]

    async def get_meeting(self, meeting_id: str) -> Optional[Dict[str, Any]]:
        """Retrieve a specific meeting by its ID."""
        async with self._get_connection() as conn:
            async with conn.execute(
                "SELECT * FROM meetings WHERE id = ? AND deleted_at IS NULL",
                (meeting_id,)
            ) as cursor:
                row = await cursor.fetchone()
                if row:
                    columns = [col[0] for col in cursor.description]
                    return {
                        col: (
                            json.loads(row[idx])
                            if col in ("attendees", "tags") and row[idx]
                            else row[idx]
                        )
                        for idx, col in enumerate(columns)
                    }
                return None

    async def update_meeting(self, meeting: Dict[str, Any]) -> None:
        """Update an existing meeting record."""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                """
                UPDATE meetings
                SET title = ?, date = ?, time = ?, attendees = ?, tags = ?,
                    content = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                """,
                (
                    meeting['title'],
                    meeting['date'],
                    meeting.get('time'),
                    json.dumps(meeting.get('attendees', [])),
                    json.dumps(meeting.get('tags', [])),
                    meeting.get('content', ""),
                    now,
                    meeting['id']
                )
            )
            await conn.commit()

        logger.info(f"Updated meeting {meeting['id']}")

    async def delete_meeting(self, meeting_id: str) -> None:
        """Soft delete a meeting by setting the deleted_at timestamp."""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                "UPDATE meetings SET deleted_at = ? WHERE id = ?",
                (now, meeting_id)
            )
            await conn.commit()

        logger.info(f"Deleted meeting {meeting_id}")

    async def create_process(self) -> str:
        """Create a new process record and return its ID."""
        process_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                """
                INSERT INTO summary_processes (id, status, created_at, updated_at, start_time)
                VALUES (?, ?, ?, ?, ?)
                """,
                (process_id, "PENDING", now, now, now)
            )
            await conn.commit()

        logger.info(f"Created process {process_id}")
        return process_id

    async def update_process(self, process_id: str, **kwargs):
        """Update process status and metadata."""
        now = datetime.utcnow().isoformat()
        updates = {
            key: json.dumps(value) if isinstance(value, dict) else value
            for key, value in kwargs.items()
        }
        updates["updated_at"] = now

        if updates.get("status") in {"COMPLETED", "FAILED"}:
            updates["end_time"] = now

        set_clause = ", ".join(f"{key} = ?" for key in updates)
        query = f"UPDATE summary_processes SET {set_clause} WHERE id = ?"

        async with self._get_connection() as conn:
            await conn.execute(query, (*updates.values(), process_id))
            await conn.commit()

        logger.info(f"Updated process {process_id} with {kwargs}")

    async def cleanup_old_processes(self, hours: int = 24) -> None:
        """Delete processes older than the given number of hours."""
        cutoff = (datetime.utcnow() - timedelta(hours=hours)).isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                "DELETE FROM summary_processes WHERE created_at < ?",
                (cutoff,)
            )
            await conn.commit()

        logger.info(f"Cleaned up processes older than {hours} hours.")

    # ------------------------------------------------
    # 1) SAVE_TRANSCRIPT: Insert transcript in 'transcripts' table
    # ------------------------------------------------
    async def save_transcript(
        self,
        process_id: str,
        transcript_text: str,
        model: str,
        model_name: str,
        chunk_size: int,
        overlap: int
    ) -> None:
        """
        Insert or replace a transcript record tied to a specific process_id.
        """
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            # If the same process_id might be reused, consider using INSERT OR REPLACE if needed
            # For strictly one process = one transcript, just use INSERT
            await conn.execute(
                """
                INSERT INTO transcripts
                    (process_id, meeting_name, transcript_text, model, model_name, chunk_size, overlap, created_at)
                VALUES
                    (?, ?, ?, ?, ?, ?, ?, ?)
            """,
                (
                    process_id,
                    None,  # meeting_name can be updated later
                    transcript_text,
                    model,
                    model_name,
                    chunk_size,
                    overlap,
                    now
                )
            )
            await conn.commit()

        logger.info(f"Saved transcript for process {process_id}")

    # ------------------------------------------------
    # 2) UPDATE_MEETING_NAME: Update 'meeting_name' field in 'transcripts' table
    # ------------------------------------------------
    async def update_meeting_name(self, process_id: str, new_meeting_name: str) -> None:
        """
        Update the meeting_name field in transcripts for the specified process_id.
        """
        async with self._get_connection() as conn:
            await conn.execute(
                """
                UPDATE transcripts
                SET meeting_name = ?
                WHERE process_id = ?
                """,
                (new_meeting_name, process_id)
            )
            await conn.commit()

        logger.info(f"Updated meeting_name for process {process_id} to '{new_meeting_name}'")
