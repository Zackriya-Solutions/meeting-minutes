use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Longest context text accepted for one level, in characters.
pub const MAX_SUMMARY_CONTEXT_CHARS: usize = 4000;

/// The level a piece of summary context applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryContextScope {
    Global,
    Folder,
    Meeting,
}

impl SummaryContextScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Folder => "folder",
            Self::Meeting => "meeting",
        }
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SummaryContextRow {
    pub context: String,
    pub use_inherited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SummaryContextRepository;

impl SummaryContextRepository {
    pub async fn get(
        pool: &SqlitePool,
        scope: SummaryContextScope,
        scope_id: &str,
    ) -> Result<Option<SummaryContextRow>, sqlx::Error> {
        sqlx::query_as::<_, SummaryContextRow>(
            "SELECT context, use_inherited, created_at, updated_at
             FROM summary_contexts WHERE scope = ? AND scope_id = ?",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .fetch_optional(pool)
        .await
    }

    /// Creates or replaces the row. The text is stored trimmed; clearing it keeps the row.
    pub async fn save(
        pool: &SqlitePool,
        scope: SummaryContextScope,
        scope_id: &str,
        context: &str,
        use_inherited: bool,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO summary_contexts (scope, scope_id, context, use_inherited, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(scope, scope_id) DO UPDATE SET
                 context = excluded.context,
                 use_inherited = excluded.use_inherited,
                 updated_at = excluded.updated_at",
        )
        .bind(scope.as_str())
        .bind(scope_id)
        .bind(context.trim())
        .bind(use_inherited)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use chrono::Utc;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;

    /// An empty in-memory database. One connection, because each in-memory
    /// connection is its own database.
    pub(crate) async fn bare_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await
            .unwrap()
    }

    /// An in-memory database with every migration applied.
    pub(crate) async fn test_pool() -> SqlitePool {
        let pool = bare_pool().await;
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    pub(crate) async fn insert_meeting(pool: &SqlitePool, id: &str) {
        let now = Utc::now();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'Test meeting', ?, ?)")
            .bind(id)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::{bare_pool, insert_meeting, test_pool};
    use super::SummaryContextScope::{Folder, Global, Meeting};
    use super::*;

    async fn saved_text(pool: &SqlitePool, scope: SummaryContextScope, scope_id: &str) -> String {
        SummaryContextRepository::get(pool, scope, scope_id).await.unwrap().unwrap().context
    }

    #[tokio::test]
    async fn missing_row_reads_as_none() {
        let pool = test_pool().await;
        assert_eq!(SummaryContextRepository::get(&pool, Global, "").await.unwrap(), None);
    }

    #[tokio::test]
    async fn first_save_creates_the_row_and_a_second_save_updates_it() {
        let pool = test_pool().await;
        SummaryContextRepository::save(&pool, Meeting, "m1", "first", true).await.unwrap();
        let first = SummaryContextRepository::get(&pool, Meeting, "m1").await.unwrap().unwrap();
        assert_eq!(first.context, "first");
        assert!(first.use_inherited);

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        SummaryContextRepository::save(&pool, Meeting, "m1", "second", false).await.unwrap();
        let second = SummaryContextRepository::get(&pool, Meeting, "m1").await.unwrap().unwrap();
        assert_eq!(second.context, "second");
        assert!(!second.use_inherited);
        assert_eq!(second.created_at, first.created_at);
        assert!(second.updated_at > first.updated_at);
    }

    #[tokio::test]
    async fn text_is_trimmed_and_whitespace_only_is_stored_empty() {
        let pool = test_pool().await;
        SummaryContextRepository::save(&pool, Global, "", "  padded \n", true).await.unwrap();
        assert_eq!(saved_text(&pool, Global, "").await, "padded");
        SummaryContextRepository::save(&pool, Global, "", " \n\t ", true).await.unwrap();
        assert_eq!(saved_text(&pool, Global, "").await, "");
    }

    #[tokio::test]
    async fn clearing_keeps_the_row() {
        let pool = test_pool().await;
        SummaryContextRepository::save(&pool, Meeting, "m1", "text", false).await.unwrap();
        SummaryContextRepository::save(&pool, Meeting, "m1", "", false).await.unwrap();
        let row = SummaryContextRepository::get(&pool, Meeting, "m1").await.unwrap().unwrap();
        assert_eq!(row.context, "");
        assert!(!row.use_inherited);
    }

    #[tokio::test]
    async fn scopes_do_not_leak_into_each_other() {
        let pool = test_pool().await;
        SummaryContextRepository::save(&pool, Global, "", "global text", true).await.unwrap();
        SummaryContextRepository::save(&pool, Folder, "shared-id", "folder text", true).await.unwrap();
        SummaryContextRepository::save(&pool, Meeting, "shared-id", "meeting text", true).await.unwrap();

        assert_eq!(saved_text(&pool, Global, "").await, "global text");
        assert_eq!(saved_text(&pool, Folder, "shared-id").await, "folder text");
        assert_eq!(saved_text(&pool, Meeting, "shared-id").await, "meeting text");
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_only_its_own_row() {
        let pool = test_pool().await;
        insert_meeting(&pool, "m1").await;
        insert_meeting(&pool, "m2").await;
        SummaryContextRepository::save(&pool, Global, "", "global text", true).await.unwrap();
        SummaryContextRepository::save(&pool, Meeting, "m1", "one", true).await.unwrap();
        SummaryContextRepository::save(&pool, Meeting, "m2", "two", true).await.unwrap();

        sqlx::query("DELETE FROM meetings WHERE id = 'm1'").execute(&pool).await.unwrap();

        assert_eq!(SummaryContextRepository::get(&pool, Meeting, "m1").await.unwrap(), None);
        assert!(SummaryContextRepository::get(&pool, Meeting, "m2").await.unwrap().is_some());
        assert!(SummaryContextRepository::get(&pool, Global, "").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn migration_applies_to_a_database_that_already_has_meetings() {
        let pool = bare_pool().await;
        sqlx::query(
            "CREATE TABLE meetings (
                id TEXT PRIMARY KEY, title TEXT NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_meeting(&pool, "existing").await;

        let migration = include_str!("../../../migrations/20261013000000_add_summary_contexts.sql");
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        // IF NOT EXISTS makes a second run harmless.
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();

        SummaryContextRepository::save(&pool, Meeting, "existing", "kept until deleted", true)
            .await
            .unwrap();
        sqlx::query("DELETE FROM meetings WHERE id = 'existing'").execute(&pool).await.unwrap();
        assert_eq!(SummaryContextRepository::get(&pool, Meeting, "existing").await.unwrap(), None);
    }
}
