import sqlite3
import logging
from typing import Dict, List, Tuple

logger = logging.getLogger(__name__)

class SchemaValidator:
    """Handles database schema validation and automatic fixes"""
    
    def __init__(self, db_path: str):
        self.db_path = db_path
    
    def validate_schema(self):
        """Validate that actual schema matches expected schema"""
        try:
            with sqlite3.connect(self.db_path) as conn:
                cursor = conn.cursor()
                
                # Get expected schema from the code
                expected_schema = self._get_expected_schema()
                
                # Validate each table
                for table_name, expected_columns in expected_schema.items():
                    self._validate_table_schema(cursor, table_name, expected_columns)
                    
        except Exception as e:
            logger.error(f"Schema validation failed: {str(e)}")
            raise

    def _get_expected_schema(self):
        """Get the expected schema from the code"""
        # This represents the schema defined in _legacy_init_db method
        return {
            'meetings': [
                ('id', 'TEXT', 'PRIMARY KEY'),
                ('title', 'TEXT', 'NOT NULL'),
                ('created_at', 'TEXT', 'NOT NULL'),
                ('updated_at', 'TEXT', 'NOT NULL')
            ],
            'transcripts': [
                ('id', 'TEXT', 'PRIMARY KEY'),
                ('meeting_id', 'TEXT', 'NOT NULL'),
                ('transcript', 'TEXT', 'NOT NULL'),
                ('timestamp', 'TEXT', 'NOT NULL'),
                ('summary', 'TEXT', ''),
                ('action_items', 'TEXT', ''),
                ('key_points', 'TEXT', '')
            ],
            'summary_processes': [
                ('meeting_id', 'TEXT', 'PRIMARY KEY'),
                ('status', 'TEXT', 'NOT NULL'),
                ('created_at', 'TEXT', 'NOT NULL'),
                ('updated_at', 'TEXT', 'NOT NULL'),
                ('error', 'TEXT', ''),
                ('result', 'TEXT', ''),
                ('start_time', 'TEXT', ''),
                ('end_time', 'TEXT', ''),
                ('chunk_count', 'INTEGER', 'DEFAULT 0'),
                ('processing_time', 'REAL', 'DEFAULT 0.0'),
                ('metadata', 'TEXT', '')
            ],
            'transcript_chunks': [
                ('meeting_id', 'TEXT', 'PRIMARY KEY'),
                ('meeting_name', 'TEXT', ''),
                ('transcript_text', 'TEXT', 'NOT NULL'),
                ('model', 'TEXT', 'NOT NULL'),
                ('model_name', 'TEXT', 'NOT NULL'),
                ('chunk_size', 'INTEGER', ''),
                ('overlap', 'INTEGER', ''),
                ('created_at', 'TEXT', 'NOT NULL')
            ],
            'settings': [
                ('id', 'TEXT', 'PRIMARY KEY'),
                ('provider', 'TEXT', 'NOT NULL'),
                ('model', 'TEXT', 'NOT NULL'),
                ('whisperModel', 'TEXT', 'NOT NULL'),
                ('groqApiKey', 'TEXT', ''),
                ('openaiApiKey', 'TEXT', ''),
                ('anthropicApiKey', 'TEXT', ''),
                ('ollamaApiKey', 'TEXT', '')
            ],
            'transcript_settings': [
                ('id', 'TEXT', 'PRIMARY KEY'),
                ('provider', 'TEXT', 'NOT NULL'),
                ('model', 'TEXT', 'NOT NULL'),
                ('whisperApiKey', 'TEXT', ''),
                ('deepgramApiKey', 'TEXT', ''),
                ('elevenLabsApiKey', 'TEXT', ''),
                ('groqApiKey', 'TEXT', ''),
                ('openaiApiKey', 'TEXT', '')
            ]
        }

    def _validate_table_schema(self, cursor, table_name: str, expected_columns: List[Tuple[str, str, str]]):
        """Validate and fix a single table's schema"""
        try:
            # Check if table exists
            cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name=?", (table_name,))
            if not cursor.fetchone():
                logger.warning(f"Table {table_name} does not exist - will be created by legacy init")
                return
            
            # Get actual columns
            cursor.execute(f"PRAGMA table_info({table_name})")
            actual_columns = {row[1]: row[2] for row in cursor.fetchall()}
            
            missing_columns = []
            
            # Check each expected column
            for col_name, col_type, col_constraints in expected_columns:
                if col_name not in actual_columns:
                    missing_columns.append((col_name, col_type))
            
            if missing_columns:
                logger.warning(f"Schema validation failed for {table_name}: missing columns {[col[0] for col in missing_columns]}")
                logger.info(f"Adding missing columns to {table_name}...")
                
                # Add each missing column
                for col_name, col_type in missing_columns:
                    cursor.execute(f"ALTER TABLE {table_name} ADD COLUMN {col_name} {col_type}")
                    logger.info(f"✅ Added missing {col_name} column to {table_name}")
            else:
                logger.info(f"✅ Schema validation passed for {table_name}")
                
        except Exception as e:
            logger.error(f"Error validating table {table_name}: {str(e)}")
            raise
