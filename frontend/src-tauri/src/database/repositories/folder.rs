use serde::{Deserialize, Serialize};
use sqlx::{Connection, Error as SqlxError, FromRow, SqlitePool};
use tracing::info;

/// Folder row joined with the number of meetings assigned to it.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FolderWithCount {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub meeting_count: i64,
}

pub struct FoldersRepository;

impl FoldersRepository {
    pub async fn get_folders(pool: &SqlitePool) -> Result<Vec<FolderWithCount>, SqlxError> {
        let folders = sqlx::query_as::<_, FolderWithCount>(
            "SELECT f.id, f.name, f.created_at, COUNT(m.id) AS meeting_count
             FROM folders f
             LEFT JOIN meetings m ON m.folder_id = f.id
             GROUP BY f.id, f.name, f.created_at
             ORDER BY f.name COLLATE NOCASE ASC",
        )
        .fetch_all(pool)
        .await?;
        Ok(folders)
    }

    pub async fn create_folder(
        pool: &SqlitePool,
        id: &str,
        name: &str,
        created_at: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("INSERT INTO folders (id, name, created_at) VALUES (?, ?, ?)")
            .bind(id)
            .bind(name)
            .bind(created_at)
            .execute(pool)
            .await?;
        info!("Created folder {} ({})", name, id);
        Ok(())
    }

    pub async fn rename_folder(
        pool: &SqlitePool,
        folder_id: &str,
        name: &str,
    ) -> Result<bool, SqlxError> {
        let result = sqlx::query("UPDATE folders SET name = ? WHERE id = ?")
            .bind(name)
            .bind(folder_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete a folder; its meetings become unfiled (folder_id = NULL).
    pub async fn delete_folder(pool: &SqlitePool, folder_id: &str) -> Result<bool, SqlxError> {
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;

        sqlx::query("UPDATE meetings SET folder_id = NULL WHERE folder_id = ?")
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        info!("Deleted folder {}", folder_id);
        Ok(result.rows_affected() > 0)
    }

    /// Assign a meeting to a folder, or unfile it with `None`.
    pub async fn set_meeting_folder(
        pool: &SqlitePool,
        meeting_id: &str,
        folder_id: Option<&str>,
    ) -> Result<bool, SqlxError> {
        let result = sqlx::query("UPDATE meetings SET folder_id = ? WHERE id = ?")
            .bind(folder_id)
            .bind(meeting_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
