// database/repositories/speaker_profile.rs
//
// CRUD for persistent voice profiles (speaker identification).
// Embeddings are stored as f32 little-endian BLOBs.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Error as SqlxError, FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct SpeakerProfileRow {
    pub id: String,
    pub name: String,
    pub embedding: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub embedding: Vec<f32>,
}

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

pub struct SpeakerProfilesRepository;

impl SpeakerProfilesRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<SpeakerProfile>, SqlxError> {
        let rows = sqlx::query_as::<_, SpeakerProfileRow>(
            "SELECT id, name, embedding FROM speaker_profiles ORDER BY name",
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SpeakerProfile {
                id: r.id,
                name: r.name,
                embedding: blob_to_embedding(&r.embedding),
            })
            .collect())
    }

    pub async fn create(
        pool: &SqlitePool,
        name: &str,
        embedding: &[f32],
    ) -> Result<String, SqlxError> {
        let id = format!("speaker-{}", Uuid::new_v4());
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO speaker_profiles (id, name, embedding, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(embedding_to_blob(embedding))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(id)
    }

    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<(), SqlxError> {
        sqlx::query("UPDATE speaker_profiles SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(Utc::now())
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), SqlxError> {
        sqlx::query("DELETE FROM speaker_profiles WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let embedding = vec![0.5f32, -1.25, 3.75, 0.0];
        let blob = embedding_to_blob(&embedding);
        assert_eq!(blob.len(), 16);
        assert_eq!(blob_to_embedding(&blob), embedding);
    }
}
