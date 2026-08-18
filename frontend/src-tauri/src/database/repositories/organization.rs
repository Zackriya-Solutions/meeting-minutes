use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectFolder {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
}

pub struct OrganizationRepository;

impl OrganizationRepository {
    pub async fn get_folders(pool: &SqlitePool) -> Result<Vec<ProjectFolder>, sqlx::Error> {
        sqlx::query_as::<_, ProjectFolder>(
            "SELECT id, name, created_at, updated_at FROM project_folders ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create_folder(
        pool: &SqlitePool,
        name: &str,
    ) -> Result<ProjectFolder, sqlx::Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(sqlx::Error::Protocol("Folder name cannot be empty".into()));
        }
        let folder = ProjectFolder {
            id: format!("project-{}", Uuid::new_v4()),
            name: name.to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        sqlx::query(
            "INSERT INTO project_folders (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&folder.id)
        .bind(&folder.name)
        .bind(&folder.created_at)
        .bind(&folder.updated_at)
        .execute(pool)
        .await?;
        Ok(folder)
    }

    pub async fn rename_folder(
        pool: &SqlitePool,
        folder_id: &str,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(sqlx::Error::Protocol("Folder name cannot be empty".into()));
        }
        let result =
            sqlx::query("UPDATE project_folders SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name)
                .bind(Utc::now().to_rfc3339())
                .bind(folder_id)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_folder(pool: &SqlitePool, folder_id: &str) -> Result<bool, sqlx::Error> {
        let mut transaction = pool.begin().await?;
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM project_folders WHERE id = ?")
            .bind(folder_id)
            .fetch_optional(&mut *transaction)
            .await?;
        if exists.is_none() {
            transaction.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE meetings SET project_folder_id = NULL, updated_at = ? WHERE project_folder_id = ?")
            .bind(Utc::now()).bind(folder_id).execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM project_folders WHERE id = ?")
            .bind(folder_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn assign_folder(
        pool: &SqlitePool,
        meeting_id: &str,
        folder_id: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("UPDATE meetings SET project_folder_id = ?, updated_at = ? WHERE id = ?")
                .bind(folder_id)
                .bind(Utc::now())
                .bind(meeting_id)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_tags_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<Tag>, sqlx::Error> {
        sqlx::query_as::<_, Tag>(
            "SELECT t.id, t.name FROM tags t JOIN meeting_tags mt ON mt.tag_id = t.id WHERE mt.meeting_id = ? ORDER BY t.name COLLATE NOCASE",
        )
        .bind(meeting_id).fetch_all(pool).await
    }

    pub async fn get_all_tags(pool: &SqlitePool) -> Result<Vec<Tag>, sqlx::Error> {
        sqlx::query_as::<_, Tag>("SELECT id, name FROM tags ORDER BY name COLLATE NOCASE")
            .fetch_all(pool)
            .await
    }

    pub async fn add_tag(
        pool: &SqlitePool,
        meeting_id: &str,
        name: &str,
    ) -> Result<Tag, sqlx::Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(sqlx::Error::Protocol("Tag name cannot be empty".into()));
        }
        let existing =
            sqlx::query_as::<_, Tag>("SELECT id, name FROM tags WHERE name = ? COLLATE NOCASE")
                .bind(name)
                .fetch_optional(pool)
                .await?;
        let tag = if let Some(tag) = existing {
            tag
        } else {
            let tag = Tag {
                id: format!("tag-{}", Uuid::new_v4()),
                name: name.to_string(),
            };
            sqlx::query("INSERT INTO tags (id, name) VALUES (?, ?)")
                .bind(&tag.id)
                .bind(&tag.name)
                .execute(pool)
                .await?;
            tag
        };
        sqlx::query("INSERT OR IGNORE INTO meeting_tags (meeting_id, tag_id) VALUES (?, ?)")
            .bind(meeting_id)
            .bind(&tag.id)
            .execute(pool)
            .await?;
        Ok(tag)
    }

    pub async fn remove_tag(
        pool: &SqlitePool,
        meeting_id: &str,
        tag_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM meeting_tags WHERE meeting_id = ? AND tag_id = ?")
            .bind(meeting_id)
            .bind(tag_id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM tags WHERE id = ? AND NOT EXISTS (SELECT 1 FROM meeting_tags WHERE tag_id = ?)")
            .bind(tag_id).bind(tag_id).execute(pool).await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn deleting_folder_returns_meetings_to_unfiled_and_keeps_tags() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('meeting-1', 'Planning', '2026-08-18T10:00:00Z', '2026-08-18T10:00:00Z')").execute(&pool).await.unwrap();
        let folder = OrganizationRepository::create_folder(&pool, "Product")
            .await
            .unwrap();
        OrganizationRepository::assign_folder(&pool, "meeting-1", Some(&folder.id))
            .await
            .unwrap();
        let tag = OrganizationRepository::add_tag(&pool, "meeting-1", "roadmap")
            .await
            .unwrap();

        assert!(OrganizationRepository::delete_folder(&pool, &folder.id)
            .await
            .unwrap());
        let assigned: Option<String> =
            sqlx::query_scalar("SELECT project_folder_id FROM meetings WHERE id = 'meeting-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(assigned, None);
        assert_eq!(
            OrganizationRepository::get_tags_for_meeting(&pool, "meeting-1")
                .await
                .unwrap()[0]
                .id,
            tag.id
        );
    }
}
