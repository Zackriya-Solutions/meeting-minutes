use sqlx::{Result, SqlitePool};
use crate::database::models::AiChatMessage;

pub struct AiChatRepository;

impl AiChatRepository {
    pub async fn get_messages_by_meeting_id(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<AiChatMessage>> {
        sqlx::query_as::<_, AiChatMessage>(
            r#"
            SELECT id, meeting_id, role, content, created_at 
            FROM ai_chat_messages 
            WHERE meeting_id = ? 
            ORDER BY created_at ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    pub async fn insert_message(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        role: &str,
        content: &str,
    ) -> Result<AiChatMessage> {
        let message = sqlx::query_as::<_, AiChatMessage>(
            r#"
            INSERT INTO ai_chat_messages (id, meeting_id, role, content)
            VALUES (?, ?, ?, ?)
            RETURNING id, meeting_id, role, content, created_at
            "#,
        )
        .bind(id)
        .bind(meeting_id)
        .bind(role)
        .bind(content)
        .fetch_one(pool)
        .await?;

        Ok(message)
    }
    
    pub async fn delete_messages_by_meeting_id(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM ai_chat_messages 
            WHERE meeting_id = ?
            "#,
        )
        .bind(meeting_id)
        .execute(pool)
        .await?;

        Ok(())
    }
}
