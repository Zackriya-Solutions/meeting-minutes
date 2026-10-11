use std::collections::HashMap;

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::org_error::{clean_name, duplicate_or, is_unique_violation, OrgError};
use crate::database::models::{Tag, TagWithCount};

pub struct TagRepository;

impl TagRepository {
    pub async fn list_with_counts(pool: &SqlitePool) -> Result<Vec<TagWithCount>, OrgError> {
        Ok(sqlx::query_as::<_, TagWithCount>(
            "SELECT t.id, t.name, COUNT(m.id) AS meeting_count
             FROM tags t
             LEFT JOIN meeting_tags mt ON mt.tag_id = t.id AND mt.deleted_at IS NULL
             LEFT JOIN meetings m ON m.id = mt.meeting_id AND m.deleted_at IS NULL
             WHERE t.deleted_at IS NULL
             GROUP BY t.id, t.name
             ORDER BY t.name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?)
    }

    async fn get_live(pool: &SqlitePool, id: &str) -> Result<Tag, OrgError> {
        sqlx::query_as::<_, Tag>(
            "SELECT id, name, created_at, updated_at FROM tags WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| OrgError::NotFound(format!("Tag not found: {}", id)))
    }

    async fn find_live_by_name(pool: &SqlitePool, name: &str) -> Result<Option<Tag>, OrgError> {
        Ok(sqlx::query_as::<_, Tag>(
            "SELECT id, name, created_at, updated_at FROM tags
             WHERE name = ? COLLATE NOCASE AND deleted_at IS NULL",
        )
        .bind(name)
        .fetch_optional(pool)
        .await?)
    }

    /// Return the live tag with this name (case-insensitive), creating it if needed.
    pub async fn create_or_get(pool: &SqlitePool, name: &str) -> Result<Tag, OrgError> {
        let name = clean_name(name)?;
        if let Some(existing) = Self::find_live_by_name(pool, &name).await? {
            return Ok(existing);
        }
        let id = format!("tag-{}", Uuid::new_v4());
        let now = Utc::now();
        let inserted = sqlx::query(
            "INSERT INTO tags (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await;
        match inserted {
            Ok(_) => Self::get_live(pool, &id).await,
            // Another caller created it between the lookup and the insert.
            Err(e) if is_unique_violation(&e) => Self::find_live_by_name(pool, &name)
                .await?
                .ok_or_else(|| OrgError::Db(e)),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<Tag, OrgError> {
        let name = clean_name(name)?;
        let result = sqlx::query(
            "UPDATE tags SET name = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&name)
        .bind(Utc::now())
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| duplicate_or(e, "tag", &name))?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Tag not found: {}", id)));
        }
        Self::get_live(pool, id).await
    }

    /// Tombstone the tag and all of its live meeting links.
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), OrgError> {
        let mut tx = pool.begin().await?;
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE tags SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Tag not found: {}", id)));
        }
        sqlx::query(
            "UPDATE meeting_tags SET deleted_at = ?, updated_at = ? WHERE tag_id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Replace a meeting's tag set. Removed links are tombstoned and re-added ones revived,
    /// so the history survives for sync. Unknown ids reject the whole call.
    pub async fn set_meeting_tags(
        pool: &SqlitePool,
        meeting_id: &str,
        tag_ids: &[String],
    ) -> Result<Vec<String>, OrgError> {
        let mut wanted: Vec<String> = Vec::new();
        for id in tag_ids {
            if !wanted.contains(id) {
                wanted.push(id.clone());
            }
        }

        let mut tx = pool.begin().await?;
        let meeting: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut *tx)
            .await?;
        if meeting.is_none() {
            return Err(OrgError::NotFound(format!("Meeting not found: {}", meeting_id)));
        }
        for id in &wanted {
            let live: Option<(i64,)> =
                sqlx::query_as("SELECT 1 FROM tags WHERE id = ? AND deleted_at IS NULL")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if live.is_none() {
                return Err(OrgError::NotFound(format!("Tag not found: {}", id)));
            }
        }

        let current: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT tag_id, deleted_at FROM meeting_tags WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&mut *tx)
                .await?;
        let now = Utc::now();
        let mut changed = false;

        for (tag_id, deleted_at) in &current {
            if deleted_at.is_none() && !wanted.contains(tag_id) {
                sqlx::query(
                    "UPDATE meeting_tags SET deleted_at = ?, updated_at = ? WHERE meeting_id = ? AND tag_id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(meeting_id)
                .bind(tag_id)
                .execute(&mut *tx)
                .await?;
                changed = true;
            }
        }
        for id in &wanted {
            match current.iter().find(|(tag_id, _)| tag_id == id) {
                None => {
                    sqlx::query(
                        "INSERT INTO meeting_tags (meeting_id, tag_id, created_at, updated_at) VALUES (?, ?, ?, ?)",
                    )
                    .bind(meeting_id)
                    .bind(id)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                    changed = true;
                }
                Some((_, Some(_))) => {
                    sqlx::query(
                        "UPDATE meeting_tags SET deleted_at = NULL, updated_at = ? WHERE meeting_id = ? AND tag_id = ?",
                    )
                    .bind(now)
                    .bind(meeting_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    changed = true;
                }
                Some((_, None)) => {}
            }
        }
        if changed {
            sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(wanted)
    }

    /// Live tag ids per meeting, for the sidebar list.
    pub async fn tag_ids_by_meeting(
        pool: &SqlitePool,
    ) -> Result<HashMap<String, Vec<String>>, OrgError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT mt.meeting_id, mt.tag_id FROM meeting_tags mt
             JOIN tags t ON t.id = mt.tag_id
             WHERE mt.deleted_at IS NULL AND t.deleted_at IS NULL",
        )
        .fetch_all(pool)
        .await?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (meeting_id, tag_id) in rows {
            map.entry(meeting_id).or_default().push(tag_id);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::test_support::{insert_meeting, test_pool};

    async fn link_rows(pool: &SqlitePool, meeting_id: &str) -> Vec<(String, Option<String>)> {
        sqlx::query_as(
            "SELECT tag_id, deleted_at FROM meeting_tags WHERE meeting_id = ? ORDER BY tag_id",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_or_get_is_idempotent_case_insensitively() {
        let pool = test_pool().await;
        let first = TagRepository::create_or_get(&pool, "todo").await.unwrap();
        let again = TagRepository::create_or_get(&pool, "  TODO ").await.unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!(again.name, "todo");
    }

    #[tokio::test]
    async fn rename_allows_own_case_variant_and_rejects_collisions() {
        let pool = test_pool().await;
        let todo = TagRepository::create_or_get(&pool, "todo").await.unwrap();
        let other = TagRepository::create_or_get(&pool, "client-x").await.unwrap();
        assert_eq!(TagRepository::rename(&pool, &todo.id, "Todo").await.unwrap().name, "Todo");
        assert!(matches!(
            TagRepository::rename(&pool, &other.id, "TODO").await,
            Err(OrgError::Duplicate(_))
        ));
    }

    #[tokio::test]
    async fn set_meeting_tags_tombstones_removed_and_revives_readded() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        let a = TagRepository::create_or_get(&pool, "a").await.unwrap();
        let b = TagRepository::create_or_get(&pool, "b").await.unwrap();

        TagRepository::set_meeting_tags(&pool, "meeting-1", &[a.id.clone(), b.id.clone()])
            .await
            .unwrap();
        TagRepository::set_meeting_tags(&pool, "meeting-1", &[a.id.clone()]).await.unwrap();
        let rows = link_rows(&pool, "meeting-1").await;
        assert!(rows.iter().any(|(id, del)| id == &a.id && del.is_none()));
        assert!(rows.iter().any(|(id, del)| id == &b.id && del.is_some()));

        TagRepository::set_meeting_tags(&pool, "meeting-1", &[a.id.clone(), b.id.clone()])
            .await
            .unwrap();
        let rows = link_rows(&pool, "meeting-1").await;
        assert_eq!(rows.len(), 2, "re-adding revives the row instead of inserting");
        assert!(rows.iter().all(|(_, del)| del.is_none()));
    }

    #[tokio::test]
    async fn set_meeting_tags_dedupes_and_rejects_unknown_without_writing() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        let a = TagRepository::create_or_get(&pool, "a").await.unwrap();

        let live = TagRepository::set_meeting_tags(&pool, "meeting-1", &[a.id.clone(), a.id.clone()])
            .await
            .unwrap();
        assert_eq!(live, vec![a.id.clone()]);

        let err = TagRepository::set_meeting_tags(
            &pool,
            "meeting-1",
            &["tag-missing".to_string()],
        )
        .await;
        assert!(matches!(err, Err(OrgError::NotFound(_))));
        let rows = link_rows(&pool, "meeting-1").await;
        assert_eq!(rows, vec![(a.id.clone(), None)], "rejected call must not remove links");
    }

    #[tokio::test]
    async fn delete_tag_tombstones_links_and_counts_skip_trash() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        insert_meeting(&pool, "meeting-2", "B", None).await;
        let a = TagRepository::create_or_get(&pool, "a").await.unwrap();
        for m in ["meeting-1", "meeting-2"] {
            TagRepository::set_meeting_tags(&pool, m, &[a.id.clone()]).await.unwrap();
        }
        sqlx::query("UPDATE meetings SET deleted_at = datetime('now') WHERE id = 'meeting-2'")
            .execute(&pool)
            .await
            .unwrap();
        let counts = TagRepository::list_with_counts(&pool).await.unwrap();
        assert_eq!(counts[0].meeting_count, 1);

        TagRepository::delete(&pool, &a.id).await.unwrap();
        assert!(TagRepository::list_with_counts(&pool).await.unwrap().is_empty());
        assert!(TagRepository::tag_ids_by_meeting(&pool).await.unwrap().is_empty());
        assert!(link_rows(&pool, "meeting-1").await.iter().all(|(_, del)| del.is_some()));
    }
}
