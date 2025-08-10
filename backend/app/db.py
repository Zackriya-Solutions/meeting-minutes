import aiosqlite
import json
import os
from datetime import datetime
from typing import Optional, Dict
import logging
from contextlib import asynccontextmanager
import sqlite3
try:
    from .schema_validator import SchemaValidator
except ImportError:
    # Handle case when running as script directly
    import sys
    import os
    sys.path.append(os.path.dirname(__file__))
    from schema_validator import SchemaValidator

logger = logging.getLogger(__name__)

class DatabaseManager:
    def __init__(self, db_path: str = None):
        if db_path is None:
            db_path = os.getenv('DATABASE_PATH', 'meeting_minutes.db')
        self.db_path = db_path
        self.schema_validator = SchemaValidator(self.db_path)
        self._init_db()

    def _init_db(self):
        """Initialize the database with legacy approach"""
        try:
            # Run legacy initialization (handles all table creation)
            logger.info("Initializing database tables...")
            self._legacy_init_db()
            
            # Validate schema integrity
            logger.info("Validating schema integrity...")
            self.schema_validator.validate_schema()
            
        except Exception as e:
            logger.error(f"Database initialization failed: {str(e)}")
            raise



    def _legacy_init_db(self):
        """Legacy database initialization (for backward compatibility)"""
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.cursor()
            
            # Create meetings table
            cursor.execute("""
                CREATE TABLE IF NOT EXISTS meetings (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )
            """)
            
            # Create transcripts table
            cursor.execute("""
                CREATE TABLE IF NOT EXISTS transcripts (
                    id TEXT PRIMARY KEY,
                    meeting_id TEXT NOT NULL,
                    transcript TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    summary TEXT,
                    action_items TEXT,
                    key_points TEXT,
                    FOREIGN KEY (meeting_id) REFERENCES meetings(id)
                )
            """)
            
            # Create summary_processes table (keeping existing functionality)
            cursor.execute("""
                CREATE TABLE IF NOT EXISTS summary_processes (
                    meeting_id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    error TEXT,
                    result TEXT,
                    start_time TEXT,
                    end_time TEXT,
                    chunk_count INTEGER DEFAULT 0,
                    processing_time REAL DEFAULT 0.0,
                    metadata TEXT,
                    FOREIGN KEY (meeting_id) REFERENCES meetings(id)
                )
            """)

            cursor.execute("""
                CREATE TABLE IF NOT EXISTS transcript_chunks (
                    meeting_id TEXT PRIMARY KEY,
                    meeting_name TEXT,
                    transcript_text TEXT NOT NULL,
                    model TEXT NOT NULL,
                    model_name TEXT NOT NULL,
                    chunk_size INTEGER,
                    overlap INTEGER,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (meeting_id) REFERENCES meetings(id)
                )
            """)

            # Create settings table
            cursor.execute("""
                CREATE TABLE IF NOT EXISTS settings (
                    id TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    model TEXT NOT NULL,
                    whisperModel TEXT NOT NULL,
                    groqApiKey TEXT,
                    openaiApiKey TEXT,
                    anthropicApiKey TEXT,
                    ollamaApiKey TEXT
                )
            """)

            # Create transcript_settings table
            cursor.execute("""
                CREATE TABLE IF NOT EXISTS transcript_settings (
                    id TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    model TEXT NOT NULL,
                    whisperApiKey TEXT,
                    deepgramApiKey TEXT,
                    elevenLabsApiKey TEXT,
                    groqApiKey TEXT,
                    openaiApiKey TEXT
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

    async def create_process(self, meeting_id: str) -> str:
        """Create a new process entry or update existing one and return its ID"""
        now = datetime.utcnow().isoformat()
        
        async with self._get_connection() as conn:
            # First try to update existing process
            await conn.execute(
                """
                UPDATE summary_processes 
                SET status = ?, updated_at = ?, start_time = ?, error = NULL, result = NULL
                WHERE meeting_id = ?
                """,
                ("PENDING", now, now, meeting_id)
            )
            
            # If no rows were updated, insert a new one
            if conn.total_changes == 0:
                await conn.execute(
                    "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, start_time) VALUES (?, ?, ?, ?, ?)",
                    (meeting_id, "PENDING", now, now, now)
                )
            
            await conn.commit()
        
        return meeting_id

    async def update_process(self, meeting_id: str, status: str, result: Optional[Dict] = None, error: Optional[str] = None, 
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
                
            params.append(meeting_id)
            query = f"UPDATE summary_processes SET {', '.join(update_fields)} WHERE meeting_id = ?"
            await conn.execute(query, params)
            await conn.commit()

    async def save_transcript(self, meeting_id: str, transcript_text: str, model: str, model_name: str, 
                            chunk_size: int, overlap: int):
        """Save transcript data"""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            # First try to update existing transcript
            await conn.execute("""
                UPDATE transcript_chunks 
                SET transcript_text = ?, model = ?, model_name = ?, chunk_size = ?, overlap = ?, created_at = ?
                WHERE meeting_id = ?
            """, (transcript_text, model, model_name, chunk_size, overlap, now, meeting_id))
            
            # If no rows were updated, insert a new one
            if conn.total_changes == 0:
                await conn.execute("""
                    INSERT INTO transcript_chunks (meeting_id, transcript_text, model, model_name, chunk_size, overlap, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                """, (meeting_id, transcript_text, model, model_name, chunk_size, overlap, now))
            
            await conn.commit()

    async def update_meeting_name(self, meeting_id: str, meeting_name: str):
        """Update meeting name in both meetings and transcript_chunks tables"""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            # Update meetings table
            await conn.execute("""
                UPDATE meetings
                SET title = ?, updated_at = ?
                WHERE id = ?
            """, (meeting_name, now, meeting_id))
            
            # Update transcript_chunks table
            await conn.execute("""
                UPDATE transcript_chunks
                SET meeting_name = ?
                WHERE meeting_id = ?
            """, (meeting_name, meeting_id))
            
            await conn.commit()

    async def get_transcript_data(self, meeting_id: str):
        """Get transcript data for a meeting"""
        async with self._get_connection() as conn:
            async with conn.execute("""
                SELECT t.*, p.status, p.result 
                FROM transcript_chunks t 
                JOIN summary_processes p ON t.meeting_id = p.meeting_id 
                WHERE t.meeting_id = ?
            """, (meeting_id,)) as cursor:
                row = await cursor.fetchone()
                if row:
                    return dict(zip([col[0] for col in cursor.description], row))
                return None

    async def save_meeting(self, meeting_id: str, title: str):
        """Save or update a meeting"""
        try:
            with sqlite3.connect(self.db_path) as conn:
                cursor = conn.cursor()
                
                # Check if meeting exists
                cursor.execute("SELECT id FROM meetings WHERE id = ? OR title = ?", (meeting_id, title))
                existing_meeting = cursor.fetchone()
                
                if not existing_meeting:
                    # Create new meeting
                    cursor.execute("""
                        INSERT INTO meetings (id, title, created_at, updated_at)
                        VALUES (?, ?, datetime('now'), datetime('now'))
                    """, (meeting_id, title))
                else:
                    # If we get here and meeting exists, throw error since we don't want duplicates
                    raise Exception(f"Meeting with ID {meeting_id} already exists")
                conn.commit()
                return True
        except Exception as e:
            logger.error(f"Error saving meeting: {str(e)}")
            raise

    async def save_meeting_transcript(self, meeting_id: str, transcript: str, timestamp: str, summary: str = "", action_items: str = "", key_points: str = ""):
        """Save a transcript for a meeting"""
        try:
            with sqlite3.connect(self.db_path) as conn:
                cursor = conn.cursor()
                
                # Save transcript
                cursor.execute("""
                    INSERT INTO transcripts (
                        meeting_id, transcript, timestamp, summary, action_items, key_points
                    ) VALUES (?, ?, ?, ?, ?, ?)
                """, (meeting_id, transcript, timestamp, summary, action_items, key_points))
                
                conn.commit()
                return True
        except Exception as e:
            logger.error(f"Error saving transcript: {str(e)}")
            raise

    async def get_meeting(self, meeting_id: str):
        """Get a meeting by ID with all its transcripts"""
        try:
            async with self._get_connection() as conn:
                # Get meeting details
                cursor = await conn.execute("""
                    SELECT id, title, created_at, updated_at
                    FROM meetings
                    WHERE id = ?
                """, (meeting_id,))
                meeting = await cursor.fetchone()
                
                if not meeting:
                    return None
                
                # Get all transcripts for this meeting
                cursor = await conn.execute("""
                    SELECT transcript, timestamp
                    FROM transcripts
                    WHERE meeting_id = ?
                """, (meeting_id,))
                transcripts = await cursor.fetchall()
                
                return {
                    'id': meeting[0],
                    'title': meeting[1],
                    'created_at': meeting[2],
                    'updated_at': meeting[3],
                    'transcripts': [{
                        'id': meeting_id,
                        'text': transcript[0],
                        'timestamp': transcript[1]
                    } for transcript in transcripts]
                }
        except Exception as e:
            logger.error(f"Error getting meeting: {str(e)}")
            raise

    async def update_meeting_title(self, meeting_id: str, new_title: str):
        """Update a meeting's title"""
        now = datetime.utcnow().isoformat()
        async with self._get_connection() as conn:
            await conn.execute("""
                UPDATE meetings
                SET title = ?, updated_at = ?
                WHERE id = ?
            """, (new_title, now, meeting_id))
            await conn.commit()

    async def get_all_meetings(self):
        """Get all meetings with basic information"""
        async with self._get_connection() as conn:
            cursor = await conn.execute("""
                SELECT id, title, created_at
                FROM meetings
                ORDER BY created_at DESC
            """)
            rows = await cursor.fetchall()
            return [{
                'id': row[0],
                'title': row[1],
                'created_at': row[2]
            } for row in rows]

    async def delete_meeting(self, meeting_id: str):
        """Delete a meeting and all its associated data"""
        async with self._get_connection() as conn:
            try:
                # Delete from transcript_chunks
                await conn.execute("DELETE FROM transcript_chunks WHERE meeting_id = ?", (meeting_id,))
                
                # Delete from summary_processes
                await conn.execute("DELETE FROM summary_processes WHERE meeting_id = ?", (meeting_id,))
                
                # Delete from transcripts
                await conn.execute("DELETE FROM transcripts WHERE meeting_id = ?", (meeting_id,))
                
                # Delete from meetings
                await conn.execute("DELETE FROM meetings WHERE id = ?", (meeting_id,))
                
                await conn.commit()
                return True
            except Exception as e:
                logger.error(f"Error deleting meeting {meeting_id}: {str(e)}")
                return False

    async def get_model_config(self):
        """Get the current model configuration"""
        async with self._get_connection() as conn:
            cursor = await conn.execute("SELECT provider, model, whisperModel FROM settings")
            row = await cursor.fetchone()
            return dict(zip([col[0] for col in cursor.description], row)) if row else None

    async def save_model_config(self, provider: str, model: str, whisperModel: str):
        """Save the model configuration"""
        async with self._get_connection() as conn:
            # Check if the configuration already exists
            cursor = await conn.execute("SELECT id FROM settings")
            existing_config = await cursor.fetchone()
            if existing_config:
                # Update existing configuration
                await conn.execute("""
                    UPDATE settings 
                    SET provider = ?, model = ?, whisperModel = ?
                    WHERE id = '1'    
                """, (provider, model, whisperModel))
            else:
                # Insert new configuration
                await conn.execute("""
                    INSERT INTO settings (id, provider, model, whisperModel)
                    VALUES (?, ?, ?, ?)
                """, ('1', provider, model, whisperModel))
            await conn.commit()


    async def save_api_key(self, api_key: str, provider: str):
        """Save the API key"""
        provider_list = ["openai", "claude", "groq", "ollama"]
        if provider not in provider_list:
            raise ValueError(f"Invalid provider: {provider}")
        if provider == "openai":
            api_key_name = "openaiApiKey"
        elif provider == "claude":
            api_key_name = "anthropicApiKey"
        elif provider == "groq":
            api_key_name = "groqApiKey"
        elif provider == "ollama":
            api_key_name = "ollamaApiKey"
        async with self._get_connection() as conn:
            await conn.execute(f"UPDATE settings SET {api_key_name} = ? WHERE id = '1'", (api_key,))
            await conn.commit()

    async def get_api_key(self, provider: str):
        """Get the API key"""
        provider_list = ["openai", "claude", "groq", "ollama"]
        if provider not in provider_list:
            raise ValueError(f"Invalid provider: {provider}")
        if provider == "openai":
            api_key_name = "openaiApiKey"
        elif provider == "claude":
            api_key_name = "anthropicApiKey"
        elif provider == "groq":
            api_key_name = "groqApiKey"
        elif provider == "ollama":
            api_key_name = "ollamaApiKey"
        async with self._get_connection() as conn:
            cursor = await conn.execute(f"SELECT {api_key_name} FROM settings WHERE id = '1'")
            row = await cursor.fetchone()
            return row[0] if row and row[0] else ""

    async def get_transcript_config(self):
        """Get the current transcript configuration"""
        async with self._get_connection() as conn:
            cursor = await conn.execute("SELECT provider, model FROM transcript_settings")
            row = await cursor.fetchone()
            if row:
                return dict(zip([col[0] for col in cursor.description], row))
            else:
                # Return default configuration if no transcript settings exist
                return {
                    "provider": "localWhisper",
                    "model": "large-v3"
                }

    async def save_transcript_config(self, provider: str, model: str):
        """Save the transcript settings"""
        async with self._get_connection() as conn:
            # Check if the configuration already exists
            cursor = await conn.execute("SELECT id FROM transcript_settings")
            existing_config = await cursor.fetchone()
            if existing_config:
                # Update existing configuration
                await conn.execute("""
                    UPDATE transcript_settings 
                    SET provider = ?, model = ?
                    WHERE id = '1'
                """, (provider, model))
            else:
                # Insert new configuration
                await conn.execute("""
                    INSERT INTO transcript_settings (id, provider, model)
                    VALUES (?, ?, ?)
                """, ('1', provider, model))
            await conn.commit()

    async def save_transcript_api_key(self, api_key: str, provider: str):
        """Save the transcript API key"""
        provider_list = ["localWhisper","deepgram","elevenLabs","groq","openai"]
        if provider not in provider_list:
            raise ValueError(f"Invalid provider: {provider}")
        if provider == "localWhisper":
            api_key_name = "whisperApiKey"
        elif provider == "deepgram":
            api_key_name = "deepgramApiKey"
        elif provider == "elevenLabs":
            api_key_name = "elevenLabsApiKey"
        elif provider == "groq":
            api_key_name = "groqApiKey"
        elif provider == "openai":
            api_key_name = "openaiApiKey"
        async with self._get_connection() as conn:
            await conn.execute(f"UPDATE transcript_settings SET {api_key_name} = ? WHERE id = '1'", (api_key,))
            await conn.commit()

    async def get_transcript_api_key(self, provider: str):
        """Get the transcript API key"""
        provider_list = ["localWhisper","deepgram","elevenLabs","groq","openai"]
        if provider not in provider_list:
            raise ValueError(f"Invalid provider: {provider}")
        if provider == "localWhisper":
            api_key_name = "whisperApiKey"
        elif provider == "deepgram":
            api_key_name = "deepgramApiKey"
        elif provider == "elevenLabs":
            api_key_name = "elevenLabsApiKey"
        elif provider == "groq":
            api_key_name = "groqApiKey"
        elif provider == "openai":
            api_key_name = "openaiApiKey"
        async with self._get_connection() as conn:
            cursor = await conn.execute(f"SELECT {api_key_name} FROM transcript_settings WHERE id = '1'")
            row = await cursor.fetchone()
            return row[0] if row and row[0] else ""

    async def search_transcripts(self, query: str):
        """Search through meeting transcripts for the given query"""
        if not query or query.strip() == "":
            return []
            
        # Convert query to lowercase for case-insensitive search
        search_query = f"%{query.lower()}%"
        
        try:
            async with self._get_connection() as conn:
                # Search in transcripts table
                cursor = await conn.execute("""
                    SELECT m.id, m.title, t.transcript, t.timestamp
                    FROM meetings m
                    JOIN transcripts t ON m.id = t.meeting_id
                    WHERE LOWER(t.transcript) LIKE ?
                    ORDER BY m.created_at DESC
                """, (search_query,))
                
                rows = await cursor.fetchall()
                
                # Also search in transcript_chunks for full transcripts
                cursor2 = await conn.execute("""
                    SELECT m.id, m.title, tc.transcript_text
                    FROM meetings m
                    JOIN transcript_chunks tc ON m.id = tc.meeting_id
                    WHERE LOWER(tc.transcript_text) LIKE ?
                    AND m.id NOT IN (SELECT DISTINCT meeting_id FROM transcripts WHERE LOWER(transcript) LIKE ?)
                    ORDER BY m.created_at DESC
                """, (search_query, search_query))
                
                chunk_rows = await cursor2.fetchall()
                
                # Format the results
                results = []
                
                # Process transcript matches
                for row in rows:
                    meeting_id, title, transcript, timestamp = row
                    
                    # Find the matching context (snippet around the match)
                    transcript_lower = transcript.lower()
                    match_index = transcript_lower.find(query.lower())
                    
                    # Extract context around the match (100 chars before and after)
                    start_index = max(0, match_index - 100)
                    end_index = min(len(transcript), match_index + len(query) + 100)
                    context = transcript[start_index:end_index]
                    
                    # Add ellipsis if we truncated the text
                    if start_index > 0:
                        context = "..." + context
                    if end_index < len(transcript):
                        context += "..."
                    
                    results.append({
                        'id': meeting_id,
                        'title': title,
                        'matchContext': context,
                        'timestamp': timestamp
                    })
                
                # Process transcript_chunks matches
                for row in chunk_rows:
                    meeting_id, title, transcript_text = row
                    
                    # Find the matching context (snippet around the match)
                    transcript_lower = transcript_text.lower()
                    match_index = transcript_lower.find(query.lower())
                    
                    # Extract context around the match (100 chars before and after)
                    start_index = max(0, match_index - 100)
                    end_index = min(len(transcript_text), match_index + len(query) + 100)
                    context = transcript_text[start_index:end_index]
                    
                    # Add ellipsis if we truncated the text
                    if start_index > 0:
                        context = "..." + context
                    if end_index < len(transcript_text):
                        context += "..."
                    
                    results.append({
                        'id': meeting_id,
                        'title': title,
                        'matchContext': context,
                        'timestamp': datetime.utcnow().isoformat()  # Use current time as fallback
                    })
                
                return results
                
        except Exception as e:
            logger.error(f"Error searching transcripts: {str(e)}")
            raise
        
    async def delete_api_key(self, provider: str):
        """Delete the API key"""
        provider_list = ["openai", "claude", "groq", "ollama"]
        if provider not in provider_list:
            raise ValueError(f"Invalid provider: {provider}")
        if provider == "openai":
            api_key_name = "openaiApiKey"
        elif provider == "claude":
            api_key_name = "anthropicApiKey"
        elif provider == "groq":
            api_key_name = "groqApiKey"
        elif provider == "ollama":
            api_key_name = "ollamaApiKey"
        async with self._get_connection() as conn:
            await conn.execute(f"UPDATE settings SET {api_key_name} = NULL WHERE id = '1'")
            await conn.commit()
    
    async def update_meeting_summary(self, meeting_id: str, summary: dict):
        """Update a meeting's summary"""
        now = datetime.utcnow().isoformat()
        try:
            async with self._get_connection() as conn:
                # Check if the meeting exists
                cursor = await conn.execute("SELECT id FROM meetings WHERE id = ?", (meeting_id,))
                meeting = await cursor.fetchone()
                
                if not meeting:
                    raise ValueError(f"Meeting with ID {meeting_id} not found")
                
                # Update the summary in the summary_processes table
                await conn.execute("""
                    UPDATE summary_processes
                    SET result = ?, updated_at = ?
                    WHERE meeting_id = ?
                """, (json.dumps(summary), now, meeting_id))
                
                # Update the meeting's updated_at timestamp
                await conn.execute("""
                    UPDATE meetings
                    SET updated_at = ?
                    WHERE id = ?
                """, (now, meeting_id))
                
                await conn.commit()
                return True
        except Exception as e:
            logger.error(f"Error updating meeting summary: {str(e)}")
            raise


   

