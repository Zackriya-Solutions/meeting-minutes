//! People that meeting speakers are linked to, and the speaker/person pairs the user rejected.

use super::speaker::{SpeakersRepository, CLEAR_SUGGESTION};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::FromRow)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A person as listed in Settings → Speakers.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PersonSummary {
    pub id: String,
    pub name: String,
    /// Meetings with a speaker linked to the person.
    pub meeting_count: i64,
    /// Creation time of the latest meeting with a linked speaker.
    pub last_seen: Option<String>,
}

/// A name as stored: trimmed, inner whitespace collapsed to single spaces. None when empty.
pub fn clean_person_name(name: &str) -> Option<String> {
    let cleaned = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!cleaned.is_empty()).then_some(cleaned)
}

/// The form names (and quotes) are compared in: lowercase, single spaces, typographic quotes as
/// plain ones.
pub fn name_key(s: &str) -> String {
    s.replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// An operation refused with a message meant for the user.
fn refused(message: impl Into<String>) -> SqlxError {
    SqlxError::Protocol(message.into())
}

/// Meetings with a speaker linked to the person.
async fn linked_meetings(conn: &mut SqliteConnection, person_id: &str) -> Result<Vec<String>, SqlxError> {
    sqlx::query_scalar("SELECT DISTINCT meeting_id FROM meeting_speakers WHERE person_id = ? ORDER BY meeting_id")
        .bind(person_id)
        .fetch_all(&mut *conn)
        .await
}

/// Meetings where a speaker is linked to, suggested or rejected the person.
async fn meetings_with_person(conn: &mut SqliteConnection, person_id: &str) -> Result<Vec<String>, SqlxError> {
    sqlx::query_scalar(
        "SELECT meeting_id FROM meeting_speakers WHERE person_id = ?1 OR suggested_person_id = ?1
         UNION SELECT meeting_id FROM speaker_rejections WHERE person_id = ?1",
    )
    .bind(person_id)
    .fetch_all(&mut *conn)
    .await
}

/// Forgets one person, or everyone with `person_id` None: linked speakers keep their name as
/// text typed by the user, and suggestions (of new names too when forgetting everyone) and
/// rejections of the forgotten go.
async fn forget_conn(conn: &mut SqliteConnection, person_id: Option<&str>) -> Result<(), SqlxError> {
    sqlx::query(
        "UPDATE meeting_speakers SET person_id = NULL, name_source = 'user'
         WHERE person_id IS NOT NULL AND (?1 IS NULL OR person_id = ?1)",
    )
    .bind(person_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(&format!("UPDATE meeting_speakers SET {CLEAR_SUGGESTION} WHERE ?1 IS NULL OR suggested_person_id = ?1"))
        .bind(person_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM speaker_rejections WHERE ?1 IS NULL OR person_id = ?1")
        .bind(person_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM people WHERE ?1 IS NULL OR id = ?1")
        .bind(person_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub struct PeopleRepository;

impl PeopleRepository {
    /// The person with this name, ignoring case (the same NOCASE rule as the unique index) and extra spaces.
    pub async fn find_by_name_conn(conn: &mut SqliteConnection, name: &str) -> Result<Option<Person>, SqlxError> {
        let Some(name) = clean_person_name(name) else {
            return Ok(None);
        };
        sqlx::query_as::<_, Person>("SELECT id, name, created_at, updated_at FROM people WHERE name = ? COLLATE NOCASE")
            .bind(name)
            .fetch_optional(&mut *conn)
            .await
    }

    pub async fn get_conn(conn: &mut SqliteConnection, id: &str) -> Result<Option<Person>, SqlxError> {
        sqlx::query_as::<_, Person>("SELECT id, name, created_at, updated_at FROM people WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
    }

    /// The person, or a "Person not found" refusal.
    async fn require_person(conn: &mut SqliteConnection, id: &str) -> Result<Person, SqlxError> {
        Self::get_conn(conn, id).await?.ok_or_else(|| refused("Person not found"))
    }

    /// The person with this name, created when there is none.
    pub async fn find_or_create_conn(conn: &mut SqliteConnection, name: &str) -> Result<Person, SqlxError> {
        let name = clean_person_name(name).ok_or_else(|| refused("A name is required"))?;
        if let Some(person) = Self::find_by_name_conn(&mut *conn, &name).await? {
            return Ok(person);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let person = Person { id: format!("person-{}", Uuid::new_v4()), name, created_at: now.clone(), updated_at: now };
        sqlx::query("INSERT INTO people (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&person.id)
            .bind(&person.name)
            .bind(&person.created_at)
            .bind(&person.updated_at)
            .execute(&mut *conn)
            .await?;
        Ok(person)
    }

    /// People with at least one linked meeting speaker, by name. People that only anchor a
    /// rejection are left out (as in `list`).
    pub async fn linked_conn(conn: &mut SqliteConnection) -> Result<Vec<Person>, SqlxError> {
        sqlx::query_as::<_, Person>(
            "SELECT p.id, p.name, p.created_at, p.updated_at FROM people p
             WHERE EXISTS (SELECT 1 FROM meeting_speakers ms WHERE ms.person_id = p.id)
             ORDER BY p.name COLLATE NOCASE",
        )
        .fetch_all(&mut *conn)
        .await
    }

    /// People with at least one linked meeting speaker, by name. People that only anchor a
    /// rejection are hidden.
    pub async fn list(pool: &SqlitePool) -> Result<Vec<PersonSummary>, SqlxError> {
        let rows: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT p.id, p.name,
                    COUNT(DISTINCT ms.meeting_id) AS meeting_count,
                    MAX(m.created_at) AS last_seen
             FROM people p
             JOIN meeting_speakers ms ON ms.person_id = p.id
             JOIN meetings m ON m.id = ms.meeting_id
             GROUP BY p.id, p.name
             ORDER BY p.name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, meeting_count, last_seen)| PersonSummary { id, name, meeting_count, last_seen })
            .collect())
    }

    /// Rename a person and every speaker linked to them (suggestions show the new name too).
    /// Returns the meetings whose display names changed.
    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<Vec<String>, SqlxError> {
        let name = clean_person_name(name).ok_or_else(|| refused("A name is required"))?;
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;
        Self::require_person(&mut tx, id).await?;
        // Checked here so a collision reads as a sentence, not as a unique-index error.
        if let Some(other) = Self::find_by_name_conn(&mut tx, &name).await?.filter(|p| p.id != id) {
            return Err(refused(format!("A person named {} already exists", other.name)));
        }
        sqlx::query("UPDATE people SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let meetings = linked_meetings(&mut tx, id).await?;
        sqlx::query("UPDATE meeting_speakers SET display_name = ? WHERE person_id = ?")
            .bind(&name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE meeting_speakers SET suggested_name = ? WHERE suggested_person_id = ?")
            .bind(&name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(meetings)
    }

    /// Fold person `from_id` into `into_id`: linked speakers are relinked and renamed,
    /// suggestions and rejections move, and `from_id` is deleted. A speaker linked to `into_id`
    /// afterwards keeps no rejection or suggestion of `into_id` (the settle step). Returns the
    /// meetings whose display names changed.
    pub async fn merge(pool: &SqlitePool, from_id: &str, into_id: &str) -> Result<Vec<String>, SqlxError> {
        if from_id == into_id {
            return Err(refused("A person cannot be merged into themselves"));
        }
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;
        Self::require_person(&mut tx, from_id).await?;
        let into = Self::require_person(&mut tx, into_id).await?;
        let meetings = linked_meetings(&mut tx, from_id).await?;
        let touched = meetings_with_person(&mut tx, from_id).await?;
        sqlx::query("UPDATE meeting_speakers SET person_id = ?, display_name = ? WHERE person_id = ?")
            .bind(&into.id)
            .bind(&into.name)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE meeting_speakers SET suggested_person_id = ?, suggested_name = ? WHERE suggested_person_id = ?")
            .bind(&into.id)
            .bind(&into.name)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO speaker_rejections (meeting_id, speaker_key, person_id)
             SELECT meeting_id, speaker_key, ? FROM speaker_rejections WHERE person_id = ?",
        )
        .bind(&into.id)
        .bind(from_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM speaker_rejections WHERE person_id = ?")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM people WHERE id = ?").bind(from_id).execute(&mut *tx).await?;
        for meeting_id in &touched {
            SpeakersRepository::settle_links_conn(&mut tx, meeting_id).await?;
        }
        tx.commit().await?;
        Ok(meetings)
    }

    /// Deletes every rejection of the meeting (a re-run writes them again under the new keys).
    pub async fn clear_rejections_conn(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), SqlxError> {
        sqlx::query("DELETE FROM speaker_rejections WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Delete a person. Linked speakers keep their name as plain text typed by the user;
    /// suggestions of the person and their rejections go. Links are cleared here rather than
    /// through the foreign keys.
    pub async fn forget(pool: &SqlitePool, id: &str) -> Result<(), SqlxError> {
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;
        Self::require_person(&mut tx, id).await?;
        forget_conn(&mut tx, Some(id)).await?;
        tx.commit().await
    }

    /// `forget` for everyone, plus every suggestion (also of new names) and every rejection.
    pub async fn forget_all(pool: &SqlitePool) -> Result<(), SqlxError> {
        let mut conn = pool.acquire().await?;
        let mut tx = conn.begin().await?;
        forget_conn(&mut tx, None).await?;
        tx.commit().await
    }

    pub async fn add_rejection_conn(
        conn: &mut SqliteConnection,
        meeting_id: &str,
        speaker_key: &str,
        person_id: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("INSERT OR IGNORE INTO speaker_rejections (meeting_id, speaker_key, person_id) VALUES (?, ?, ?)")
            .bind(meeting_id)
            .bind(speaker_key)
            .bind(person_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    pub async fn remove_rejection_conn(
        conn: &mut SqliteConnection,
        meeting_id: &str,
        speaker_key: &str,
        person_id: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("DELETE FROM speaker_rejections WHERE meeting_id = ? AND speaker_key = ? AND person_id = ?")
            .bind(meeting_id)
            .bind(speaker_key)
            .bind(person_id)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// (speaker_key, person_id) pairs rejected in the meeting.
    pub async fn rejections_conn(conn: &mut SqliteConnection, meeting_id: &str) -> Result<HashSet<(String, String)>, SqlxError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT speaker_key, person_id FROM speaker_rejections WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&mut *conn)
                .await?;
        Ok(rows.into_iter().collect())
    }

    /// (speaker_key, person name) pairs rejected in the meeting.
    pub async fn rejected_names_conn(conn: &mut SqliteConnection, meeting_id: &str) -> Result<HashSet<(String, String)>, SqlxError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT r.speaker_key, p.name FROM speaker_rejections r JOIN people p ON p.id = r.person_id WHERE r.meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows.into_iter().collect())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::speaker::{embedding_to_blob, NameSource, SpeakerLink};
    use crate::database::test_support::{migrated_pool, people_count, seed_meeting, seed_person, stored_speaker};

    async fn people_count_conn(conn: &mut SqliteConnection) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM people").fetch_one(&mut *conn).await.unwrap()
    }

    /// One meeting speaker; `voiced` stores a voice centroid (a hand-made speaker has none).
    async fn add_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
        key: &str,
        name: Option<&str>,
        person_id: Option<&str>,
        source: Option<NameSource>,
        voiced: bool,
    ) {
        sqlx::query(
            "INSERT INTO meeting_speakers (meeting_id, speaker_key, display_name, embedding, person_id, name_source, created_at)
             VALUES (?, ?, ?, ?, ?, ?, '2026-10-05T10:00:00Z')",
        )
        .bind(meeting_id)
        .bind(key)
        .bind(name)
        .bind(voiced.then(|| embedding_to_blob(&[1.0, 0.0])))
        .bind(person_id)
        .bind(source.map(NameSource::as_str))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn suggest(pool: &SqlitePool, meeting_id: &str, key: &str, person_id: Option<&str>, name: &str) {
        sqlx::query(
            "UPDATE meeting_speakers SET suggested_person_id = ?, suggested_name = ?, suggestion_source = 'conversation',
                 suggestion_reason = 'addressed as someone at 00:10'
             WHERE meeting_id = ? AND speaker_key = ?",
        )
        .bind(person_id)
        .bind(name)
        .bind(meeting_id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn reject(pool: &SqlitePool, meeting_id: &str, key: &str, person_id: &str) {
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::add_rejection_conn(&mut conn, meeting_id, key, person_id).await.unwrap();
    }

    async fn rejections(pool: &SqlitePool, meeting_id: &str) -> HashSet<(String, String)> {
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::rejections_conn(&mut conn, meeting_id).await.unwrap()
    }

    async fn person(pool: &SqlitePool, id: &str) -> Option<Person> {
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::get_conn(&mut conn, id).await.unwrap()
    }

    async fn set_created(pool: &SqlitePool, meeting_id: &str, at: &str) {
        sqlx::query("UPDATE meetings SET created_at = ? WHERE id = ?")
            .bind(at)
            .bind(meeting_id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn pair(key: &str, person_id: &str) -> (String, String) {
        (key.to_string(), person_id.to_string())
    }

    #[test]
    fn clean_person_name_trims_and_collapses_spaces() {
        assert_eq!(clean_person_name("  Mary   Ann "), Some("Mary Ann".to_string()));
        assert_eq!(clean_person_name("Noah"), Some("Noah".to_string()));
        assert_eq!(clean_person_name(" \t "), None);
    }

    #[tokio::test]
    async fn find_or_create_reuses_case_insensitive_match() {
        let pool = migrated_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let noah = PeopleRepository::find_or_create_conn(&mut conn, "Noah").await.unwrap();
        assert!(noah.id.starts_with("person-"));
        assert_eq!(noah.name, "Noah");
        let again = PeopleRepository::find_or_create_conn(&mut conn, "NOAH").await.unwrap();
        assert_eq!(again, noah);
        assert_eq!(people_count_conn(&mut conn).await, 1);
        assert!(PeopleRepository::find_or_create_conn(&mut conn, "   ").await.is_err());
    }

    #[tokio::test]
    async fn typing_an_existing_name_links_case_insensitively() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        let mut conn = pool.acquire().await.unwrap();
        let found = PeopleRepository::find_by_name_conn(&mut conn, "  noah ").await.unwrap();
        assert_eq!(found.map(|p| p.id).as_deref(), Some("person-noah"));
        let linked = PeopleRepository::find_or_create_conn(&mut conn, "  noah ").await.unwrap();
        assert_eq!((linked.id.as_str(), linked.name.as_str()), ("person-noah", "Noah"));
        assert_eq!(people_count_conn(&mut conn).await, 1, "no second person");
    }

    #[tokio::test]
    async fn renaming_to_own_name_in_other_case_is_allowed() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;

        let changed = PeopleRepository::rename(&pool, "person-noah", "noah").await.unwrap();

        assert_eq!(changed, vec!["m1".to_string()]);
        assert_eq!(person(&pool, "person-noah").await.unwrap().name, "noah");
        assert_eq!(stored_speaker(&pool, "m1", "spk_0").await.display_name.as_deref(), Some("noah"));
    }

    #[tokio::test]
    async fn renaming_to_another_persons_name_is_refused() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-ana", "Ana").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Ana"), Some("person-ana"), Some(NameSource::User), true).await;

        match PeopleRepository::rename(&pool, "person-ana", " noah ").await {
            Err(SqlxError::Protocol(message)) => assert_eq!(message, "A person named Noah already exists"),
            other => panic!("expected a readable refusal, got {other:?}"),
        }
        assert_eq!(person(&pool, "person-ana").await.unwrap().name, "Ana");
        assert_eq!(stored_speaker(&pool, "m1", "spk_0").await.display_name.as_deref(), Some("Ana"));
        assert_eq!(people_count(&pool).await, 2);
    }

    #[tokio::test]
    async fn rename_updates_every_linked_speaker() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        for id in ["m1", "m2", "m3"] {
            seed_meeting(&pool, id, &[]).await;
        }
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m2", "spk_1", Some("Noah"), Some("person-noah"), Some(NameSource::Voice), true).await;
        add_speaker(&pool, "m3", "spk_0", None, None, None, true).await;
        suggest(&pool, "m3", "spk_0", Some("person-noah"), "Noah").await;

        let changed = PeopleRepository::rename(&pool, "person-noah", "Noah Parker").await.unwrap();

        assert_eq!(changed, vec!["m1".to_string(), "m2".to_string()]);
        assert_eq!(stored_speaker(&pool, "m1", "spk_0").await.display_name.as_deref(), Some("Noah Parker"));
        assert_eq!(stored_speaker(&pool, "m2", "spk_1").await.display_name.as_deref(), Some("Noah Parker"));
        assert_eq!(stored_speaker(&pool, "m3", "spk_0").await.link.suggested_name.as_deref(), Some("Noah Parker"));
    }

    #[tokio::test]
    async fn merge_relinks_renames_and_deletes() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-noa", "Noa").await;
        seed_meeting(&pool, "m1", &[]).await;
        seed_meeting(&pool, "m2", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m2", "spk_0", Some("Noa"), Some("person-noa"), Some(NameSource::User), true).await;

        let changed = PeopleRepository::merge(&pool, "person-noa", "person-noah").await.unwrap();

        assert_eq!(changed, vec!["m2".to_string()]);
        let moved = stored_speaker(&pool, "m2", "spk_0").await;
        assert_eq!(moved.display_name.as_deref(), Some("Noah"));
        assert_eq!(moved.link.person_id.as_deref(), Some("person-noah"));
        assert_eq!(moved.link.name_source, Some(NameSource::User));
        assert!(person(&pool, "person-noa").await.is_none());
    }

    #[tokio::test]
    async fn merge_moves_suggestions_and_rejections() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-noa", "Noa").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", None, None, None, true).await;
        add_speaker(&pool, "m1", "spk_1", None, None, None, true).await;
        suggest(&pool, "m1", "spk_0", Some("person-noa"), "Noa").await;
        reject(&pool, "m1", "spk_1", "person-noa").await;
        reject(&pool, "m1", "spk_1", "person-noah").await;

        let changed = PeopleRepository::merge(&pool, "person-noa", "person-noah").await.unwrap();

        assert!(changed.is_empty(), "no speaker was linked to Noa");
        let suggested = stored_speaker(&pool, "m1", "spk_0").await.link;
        assert_eq!(suggested.suggested_person_id.as_deref(), Some("person-noah"));
        assert_eq!(suggested.suggested_name.as_deref(), Some("Noah"));
        assert_eq!(rejections(&pool, "m1").await, HashSet::from([pair("spk_1", "person-noah")]));
    }

    #[tokio::test]
    async fn merge_never_leaves_a_speaker_rejecting_its_own_person() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-noa", "Noa").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Noa"), Some("person-noa"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m1", "spk_1", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;
        // spk_0 (now relinked to Noah) rejected Noah; spk_1 (Noah) rejected and is suggested Noa.
        reject(&pool, "m1", "spk_0", "person-noah").await;
        reject(&pool, "m1", "spk_1", "person-noa").await;
        suggest(&pool, "m1", "spk_1", Some("person-noa"), "Noa").await;

        PeopleRepository::merge(&pool, "person-noa", "person-noah").await.unwrap();

        assert_eq!(stored_speaker(&pool, "m1", "spk_0").await.link.person_id.as_deref(), Some("person-noah"));
        assert!(rejections(&pool, "m1").await.is_empty());
        assert!(!stored_speaker(&pool, "m1", "spk_1").await.link.has_suggestion(), "a suggestion of its own person says nothing");
    }

    #[tokio::test]
    async fn merge_into_self_is_refused() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        assert!(PeopleRepository::merge(&pool, "person-noah", "person-noah").await.is_err());
        assert!(PeopleRepository::merge(&pool, "person-missing", "person-noah").await.is_err());
        assert!(person(&pool, "person-noah").await.is_some());
    }

    #[tokio::test]
    async fn forget_keeps_display_names_as_user_text() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::Voice), true).await;

        PeopleRepository::forget(&pool, "person-noah").await.unwrap();

        let s = stored_speaker(&pool, "m1", "spk_0").await;
        assert_eq!(s.display_name.as_deref(), Some("Noah"));
        assert_eq!(s.link.person_id, None);
        assert_eq!(s.link.name_source, Some(NameSource::User));
        assert!(person(&pool, "person-noah").await.is_none());
    }

    #[tokio::test]
    async fn forget_clears_suggestions_and_rejections() {
        let pool = migrated_pool().await;
        // The clean-up must not rely on the cascade. One connection, so the pragma holds.
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-ana", "Ana").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", None, None, None, true).await;
        add_speaker(&pool, "m1", "spk_1", None, None, None, true).await;
        suggest(&pool, "m1", "spk_0", Some("person-noah"), "Noah").await;
        suggest(&pool, "m1", "spk_1", Some("person-ana"), "Ana").await;
        reject(&pool, "m1", "spk_0", "person-ana").await;
        reject(&pool, "m1", "spk_1", "person-noah").await;

        PeopleRepository::forget(&pool, "person-noah").await.unwrap();

        assert_eq!(stored_speaker(&pool, "m1", "spk_0").await.link, SpeakerLink::default());
        assert_eq!(stored_speaker(&pool, "m1", "spk_1").await.link.suggested_person_id.as_deref(), Some("person-ana"));
        assert_eq!(rejections(&pool, "m1").await, HashSet::from([pair("spk_0", "person-ana")]));
    }

    #[tokio::test]
    async fn forget_all_clears_people_suggestions_and_rejections() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_meeting(&pool, "m1", &[]).await;
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m1", "spk_1", None, None, None, true).await;
        suggest(&pool, "m1", "spk_1", None, "Priya").await;
        reject(&pool, "m1", "spk_1", "person-noah").await;

        PeopleRepository::forget_all(&pool).await.unwrap();

        assert_eq!(people_count(&pool).await, 0);
        let named = stored_speaker(&pool, "m1", "spk_0").await;
        assert_eq!(named.display_name.as_deref(), Some("Noah"));
        assert_eq!(named.link.person_id, None);
        assert_eq!(named.link.name_source, Some(NameSource::User));
        assert_eq!(stored_speaker(&pool, "m1", "spk_1").await.link, SpeakerLink::default(), "suggestions of new names go too");
        assert!(rejections(&pool, "m1").await.is_empty());
    }

    #[tokio::test]
    async fn clear_rejections_empties_one_meeting() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_meeting(&pool, "m1", &[]).await;
        seed_meeting(&pool, "m2", &[]).await;
        reject(&pool, "m1", "spk_0", "person-noah").await;
        reject(&pool, "m2", "spk_0", "person-noah").await;

        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::clear_rejections_conn(&mut conn, "m1").await.unwrap();
        drop(conn);

        assert!(rejections(&pool, "m1").await.is_empty());
        assert_eq!(rejections(&pool, "m2").await, HashSet::from([pair("spk_0", "person-noah")]));
    }

    #[tokio::test]
    async fn list_counts_meetings_and_last_seen() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-ana", "Ana").await;
        seed_meeting(&pool, "m1", &[]).await;
        seed_meeting(&pool, "m2", &[]).await;
        set_created(&pool, "m1", "2026-10-01T09:00:00+00:00").await;
        set_created(&pool, "m2", "2026-10-03T09:00:00+00:00").await;
        add_speaker(&pool, "m1", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m2", "spk_0", Some("Noah"), Some("person-noah"), Some(NameSource::Voice), true).await;
        add_speaker(&pool, "m2", "spk_1", Some("Noah"), Some("person-noah"), Some(NameSource::User), false).await;
        // Ana only anchors a rejection.
        reject(&pool, "m2", "spk_2", "person-ana").await;

        let people = PeopleRepository::list(&pool).await.unwrap();

        assert_eq!(
            people,
            vec![PersonSummary {
                id: "person-noah".into(),
                name: "Noah".into(),
                meeting_count: 2,
                last_seen: Some("2026-10-03T09:00:00+00:00".into()),
            }]
        );
    }

    #[tokio::test]
    async fn linked_people_exclude_rejection_only_people() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "m1", &[]).await;
        for (id, name) in [("person-zoe", "Zoe"), ("person-ana", "ana"), ("person-bea", "Bea")] {
            seed_person(&pool, id, name).await;
        }
        add_speaker(&pool, "m1", "spk_0", Some("ana"), Some("person-ana"), Some(NameSource::User), true).await;
        add_speaker(&pool, "m1", "spk_1", Some("Bea"), Some("person-bea"), Some(NameSource::Voice), true).await;
        reject(&pool, "m1", "spk_2", "person-zoe").await;
        let mut conn = pool.acquire().await.unwrap();
        let names: Vec<String> = PeopleRepository::linked_conn(&mut conn).await.unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["ana", "Bea"]);
    }

    #[tokio::test]
    async fn rejected_names_pair_each_speaker_with_the_rejected_name() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "m1", &[]).await;
        seed_meeting(&pool, "m2", &[]).await;
        seed_person(&pool, "person-zoe", "Zoe Q").await;
        seed_person(&pool, "person-ana", "Ana").await;
        reject(&pool, "m1", "spk_2", "person-zoe").await;
        reject(&pool, "m1", "spk_3", "person-ana").await;
        reject(&pool, "m2", "spk_2", "person-ana").await;
        let mut conn = pool.acquire().await.unwrap();
        let pairs = PeopleRepository::rejected_names_conn(&mut conn, "m1").await.unwrap();
        assert_eq!(
            pairs,
            HashSet::from([("spk_2".to_string(), "Zoe Q".to_string()), ("spk_3".to_string(), "Ana".to_string())])
        );
    }
}
