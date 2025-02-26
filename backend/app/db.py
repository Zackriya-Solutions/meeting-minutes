import aiosqlite
import json
from datetime import datetime, timedelta
import uuid
from typing import Optional, List, Dict, Any
import logging
import asyncio
from contextlib import asynccontextmanager

logger = logging.getLogger(__name__)

class DatabaseManager:
    def __init__(self, db_path: str = "summaries.db"):
        self.db_path = db_path
        self._init_db()

    def _init_db(self):
        """Initialize the database with required tables"""
        import sqlite3  # Use sync sqlite3 for initialization only
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.cursor()
            cursor.execute("""
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
                )
            """)
            cursor.execute("""
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
                )
            """)

            # Add meetings table if it does not exist
            cursor.execute("""
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                date TEXT NOT NULL,
                time TEXT,
                attendees TEXT,  -- JSON array of attendees
                tags TEXT,       -- JSON array of tags
                content TEXT,    -- Meeting content/summary
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT  -- Soft delete support
            )
        """)
            conn.commit()

    @asynccontextmanager
    async def _get_connection(self):
        """Get a new database connection"""
        conn = await aiosqlite.connect(self.db_path)
        try:
            yield conn
        finally:
            await conn.close()

    async def create_meeting(self, title: str, date: str, time: Optional[str] = None, 
                           attendees: List[str] = None, tags: List[str] = None, 
                           content: str = None) -> str:
        """Create a new meeting record"""
        meeting_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        
        async with self._get_connection() as conn:
            await conn.execute("""
                INSERT INTO meetings (id, title, date, time, attendees, tags, content, 
                                    created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                meeting_id, title, date, time,
                json.dumps(attendees or []),
                json.dumps(tags or []),
                content or "",
                now, now
            ))
            await conn.commit()
        return meeting_id

    async def get_meetings(self):
        """Get all non-deleted meetings"""
        async with self._get_connection() as conn:
            async with conn.execute(
                "SELECT * FROM meetings WHERE deleted_at IS NULL ORDER BY date DESC"
            ) as cursor:
                rows = await cursor.fetchall()
                columns = [col[0] for col in cursor.description]
                return [
                    {
                        **dict(zip(columns, row)),
                        'attendees': json.loads(row[4]) if row[4] else [],
                        'tags': json.loads(row[5]) if row[5] else []
                    }
                    for row in rows
                ]

    async def get_meeting(self, meeting_id: str):
        """Get a specific meeting by ID"""
        async with self._get_connection() as conn:
            async with conn.execute(
                "SELECT * FROM meetings WHERE id = ? AND deleted_at IS NULL",
                (meeting_id,)
            ) as cursor:
                row = await cursor.fetchone()
                if row:
                    columns = [col[0] for col in cursor.description]
                    result = dict(zip(columns, row))
                    result['attendees'] = json.loads(result['attendees']) if result['attendees'] else []
                    result['tags'] = json.loads(result['tags']) if result['tags'] else []
                    return result
                return None

    async def delete_meeting(self, meeting_id: str):
        """Soft delete a meeting"""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute(
                "UPDATE meetings SET deleted_at = ? WHERE id = ?",
                (now, meeting_id)
            )
            await conn.commit()

    async def create_process(self) -> str:
        """Create a new process entry and return its ID"""
        process_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        
        async with self._get_connection() as conn:
            await conn.execute(
                "INSERT INTO summary_processes (id, status, created_at, updated_at, start_time) VALUES (?, ?, ?, ?, ?)",
                (process_id, "PENDING", now, now, now)
            )
            await conn.commit()
        
        return process_id

    async def update_process(self, process_id: str, status: str, result: Optional[Dict] = None, error: Optional[str] = None, 
                           chunk_count: Optional[int] = None, processing_time: Optional[float] = None, 
                           metadata: Optional[Dict] = None):
        """Update a process status and result"""
        now = datetime.utcnow().isoformat()
        
        async with self._get_connection() as conn:
            update_fields = ["status = ?", "updated_at = ?"]
            params = [status, now]
            
            if result:
                update_fields.append("result = ?")
                params.append(json.dumps(result))
            if error:
                update_fields.append("error = ?")
                params.append(error)
            if chunk_count is not None:
                update_fields.append("chunk_count = ?")
                params.append(chunk_count)
            if processing_time is not None:
                update_fields.append("processing_time = ?")
                params.append(processing_time)
            if metadata:
                update_fields.append("metadata = ?")
                params.append(json.dumps(metadata))
            if status == 'COMPLETED' or status == 'FAILED':
                update_fields.append("end_time = ?")
                params.append(now)
                
            params.append(process_id)
            query = f"UPDATE summary_processes SET {', '.join(update_fields)} WHERE id = ?"
            await conn.execute(query, params)
            await conn.commit()

    async def get_process(self, process_id: str) -> Optional[Dict[str, Any]]:
        """Get a process by its ID"""
        async with self._get_connection() as conn:
            async with conn.execute(
                "SELECT id, status, created_at, updated_at, result, error, start_time, end_time, chunk_count, processing_time, metadata FROM summary_processes WHERE id = ?",
                (process_id,)
            ) as cursor:
                row = await cursor.fetchone()
                
                if not row:
                    return None
                    
                result = {
                    "id": row[0],
                    "status": row[1],
                    "created_at": row[2],
                    "updated_at": row[3],
                    "start_time": row[6],
                    "end_time": row[7],
                    "chunk_count": row[8],
                    "processing_time": row[9]
                }
                
                if row[4]:  # result
                    result["result"] = json.loads(row[4])
                if row[5]:  # error
                    result["error"] = row[5]
                if row[10]:  # metadata
                    result["metadata"] = json.loads(row[10])
                    
                return result

    async def save_transcript(self, process_id: str, transcript_text: str, model: str, model_name: str, 
                            chunk_size: int, overlap: int):
        """Save transcript data"""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute("""
                INSERT INTO transcripts (process_id, transcript_text, model, model_name, chunk_size, overlap, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
            """, (process_id, transcript_text, model, model_name, chunk_size, overlap, now))
            await conn.commit()

    async def update_meeting_name(self, process_id: str, meeting_name: str):
        """Update meeting name for a transcript"""
        async with self._get_connection() as conn:
            await conn.execute("""
                UPDATE transcripts SET meeting_name = ? WHERE process_id = ?
            """, (meeting_name, process_id))
            await conn.commit()

    async def get_transcript_data(self, process_id: str):
        """Get transcript data for a process"""
        async with self._get_connection() as conn:
            async with conn.execute("""
                SELECT t.*, p.status, p.result 
                FROM transcripts t 
                JOIN summary_processes p ON t.process_id = p.id 
                WHERE t.process_id = ?
            """, (process_id,)) as cursor:
                row = await cursor.fetchone()
                if row:
                    return dict(zip([col[0] for col in cursor.description], row))
                return None

    async def cleanup_old_processes(self, hours: int = 24):
        """Clean up processes older than specified hours"""
        cutoff = (datetime.utcnow() - timedelta(hours=hours)).isoformat()
        
        async with self._get_connection() as conn:
            await conn.execute(
                "DELETE FROM summary_processes WHERE created_at < ?",
                (cutoff,)
            )
            await conn.commit()
