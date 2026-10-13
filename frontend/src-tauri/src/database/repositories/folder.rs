use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::org_error::{clean_name, duplicate_or, OrgError};
use crate::database::models::Folder;

const FOLDER_COLUMNS: &str = "id, name, parent_id, sort_order, created_at, updated_at";

pub struct FolderRepository;

impl FolderRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Folder>, OrgError> {
        let sql = format!(
            "SELECT {} FROM folders WHERE deleted_at IS NULL ORDER BY name COLLATE NOCASE",
            FOLDER_COLUMNS
        );
        Ok(sqlx::query_as::<_, Folder>(&sql).fetch_all(pool).await?)
    }

    pub async fn get_live(pool: &SqlitePool, id: &str) -> Result<Folder, OrgError> {
        let sql = format!(
            "SELECT {} FROM folders WHERE id = ? AND deleted_at IS NULL",
            FOLDER_COLUMNS
        );
        sqlx::query_as::<_, Folder>(&sql)
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| OrgError::NotFound(format!("Folder not found: {}", id)))
    }

    pub async fn create(pool: &SqlitePool, name: &str) -> Result<Folder, OrgError> {
        let name = clean_name(name)?;
        let id = format!("folder-{}", Uuid::new_v4());
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO folders (id, name, parent_id, sort_order, created_at, updated_at)
             VALUES (?, ?, NULL, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| duplicate_or(e, "folder", &name))?;
        Self::get_live(pool, &id).await
    }

    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<Folder, OrgError> {
        let name = clean_name(name)?;
        let result = sqlx::query(
            "UPDATE folders SET name = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&name)
        .bind(Utc::now())
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| duplicate_or(e, "folder", &name))?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Folder not found: {}", id)));
        }
        Self::get_live(pool, id).await
    }

    /// Tombstone the folder and move its meetings (including trashed ones) to Unfiled.
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, OrgError> {
        let mut tx = pool.begin().await?;
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE folders SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Folder not found: {}", id)));
        }
        let moved = sqlx::query("UPDATE meetings SET folder_id = NULL, updated_at = ? WHERE folder_id = ?")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::test_support::{insert_meeting, test_pool};

    #[tokio::test]
    async fn create_lists_folders_alphabetically() {
        let pool = test_pool().await;
        FolderRepository::create(&pool, "work").await.unwrap();
        FolderRepository::create(&pool, "  Archive ").await.unwrap();
        FolderRepository::create(&pool, "personal").await.unwrap();

        let names: Vec<String> = FolderRepository::list(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, vec!["Archive", "personal", "work"]);
    }

    #[tokio::test]
    async fn create_rejects_live_duplicate_and_empty_names() {
        let pool = test_pool().await;
        FolderRepository::create(&pool, "Work").await.unwrap();
        assert!(matches!(
            FolderRepository::create(&pool, "work").await,
            Err(OrgError::Duplicate(_))
        ));
        assert!(matches!(
            FolderRepository::create(&pool, "   ").await,
            Err(OrgError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn rename_to_case_variant_of_own_name_succeeds() {
        let pool = test_pool().await;
        let folder = FolderRepository::create(&pool, "work").await.unwrap();
        let renamed = FolderRepository::rename(&pool, &folder.id, "Work").await.unwrap();
        assert_eq!(renamed.name, "Work");
    }

    #[tokio::test]
    async fn rename_rejects_collision_with_other_folder() {
        let pool = test_pool().await;
        FolderRepository::create(&pool, "Work").await.unwrap();
        let other = FolderRepository::create(&pool, "Home").await.unwrap();
        assert!(matches!(
            FolderRepository::rename(&pool, &other.id, "WORK").await,
            Err(OrgError::Duplicate(_))
        ));
    }

    #[tokio::test]
    async fn delete_moves_meetings_to_unfiled_and_tombstones_folder() {
        let pool = test_pool().await;
        let folder = FolderRepository::create(&pool, "Work").await.unwrap();
        insert_meeting(&pool, "meeting-1", "A", None).await;
        insert_meeting(&pool, "meeting-2", "B", None).await;
        sqlx::query("UPDATE meetings SET folder_id = ?")
            .bind(&folder.id)
            .execute(&pool)
            .await
            .unwrap();

        let moved = FolderRepository::delete(&pool, &folder.id).await.unwrap();
        assert_eq!(moved, 2);

        let unfiled: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM meetings WHERE folder_id IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unfiled.0, 2);

        let tombstone: (Option<String>,) =
            sqlx::query_as("SELECT deleted_at FROM folders WHERE id = ?")
                .bind(&folder.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(tombstone.0.is_some());
        assert!(FolderRepository::list(&pool).await.unwrap().is_empty());
        assert!(matches!(
            FolderRepository::delete(&pool, &folder.id).await,
            Err(OrgError::NotFound(_))
        ));
    }
}
