//! People a meeting is known to involve, seeded from a calendar invitation.
//!
//! This is not an address book: the rows belong to one meeting, carry no contact
//! details beyond the name the invitation showed, and disappear with the meeting.
//! Speaker naming reads them so an invited person can be recognized even when nobody
//! in the room ever says their name.

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Where a participant came from. Only calendar invitations are recorded today.
pub const SOURCE_OUTLOOK_CALENDAR: &str = "outlook_calendar";

/// A participant list longer than this is a distribution list, not a meeting, and is
/// no use to speaker naming.
pub const MAX_PARTICIPANTS: usize = 40;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingParticipant {
    pub name: String,
    pub source: String,
}

/// Fold a name to the form the uniqueness rule compares: case and inner spacing are
/// not differences between people.
fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub struct ParticipantsRepository;

impl ParticipantsRepository {
    /// Add the given names to a meeting, keeping any that are already there.
    ///
    /// Re-recording the same calendar entry, or attaching a list twice, must not
    /// duplicate anyone, so a repeated name is ignored rather than being an error.
    /// Returns how many rows were inserted.
    pub async fn add(
        pool: &SqlitePool,
        meeting_id: &str,
        names: &[String],
        source: &str,
    ) -> Result<usize, sqlx::Error> {
        let mut inserted = 0;
        for name in names.iter().take(MAX_PARTICIPANTS) {
            let display_name = name.trim();
            if display_name.is_empty() {
                continue;
            }
            let affected = sqlx::query(
                "INSERT OR IGNORE INTO meeting_participants \
                   (meeting_id, display_name, normalized_name, source) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(meeting_id)
            .bind(display_name)
            .bind(normalize(display_name))
            .bind(source)
            .execute(pool)
            .await?
            .rows_affected();
            inserted += affected as usize;
        }
        Ok(inserted)
    }

    /// Everyone recorded for a meeting, in the order they were added (organizer first,
    /// as the invitation listed them).
    pub async fn list(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingParticipant>, sqlx::Error> {
        sqlx::query_as::<_, MeetingParticipant>(
            "SELECT display_name AS name, source FROM meeting_participants \
             WHERE meeting_id = ? ORDER BY id",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Just the names, for prompts and hints.
    pub async fn names(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(Self::list(pool, meeting_id)
            .await?
            .into_iter()
            .map(|participant| participant.name)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_meeting() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, \
             created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE meeting_participants (\
               id INTEGER PRIMARY KEY, \
               meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE, \
               display_name TEXT NOT NULL, \
               normalized_name TEXT NOT NULL, \
               source TEXT NOT NULL DEFAULT 'outlook_calendar', \
               created_at TEXT NOT NULL DEFAULT (datetime('now')), \
               UNIQUE(meeting_id, normalized_name))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at) \
             VALUES ('m1', 'Планирование спринта', datetime('now'), datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn keeps_the_invitation_order_and_ignores_repeats() {
        let pool = pool_with_meeting().await;
        let invited = vec![
            "Андрей Евлампиев".to_string(),
            "Мария Петрова".to_string(),
        ];
        assert_eq!(
            ParticipantsRepository::add(&pool, "m1", &invited, SOURCE_OUTLOOK_CALENDAR)
                .await
                .unwrap(),
            2
        );

        // The same people again, spelled differently: still the same two participants.
        let repeated = vec![
            "андрей   евлампиев".to_string(),
            "Мария Петрова".to_string(),
            "Гость".to_string(),
        ];
        assert_eq!(
            ParticipantsRepository::add(&pool, "m1", &repeated, SOURCE_OUTLOOK_CALENDAR)
                .await
                .unwrap(),
            1
        );

        assert_eq!(
            ParticipantsRepository::names(&pool, "m1").await.unwrap(),
            vec![
                "Андрей Евлампиев".to_string(),
                "Мария Петрова".to_string(),
                "Гость".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn skips_blanks_and_caps_a_distribution_list() {
        let pool = pool_with_meeting().await;
        let mut invited = vec!["   ".to_string(), "".to_string()];
        invited.extend((0..100).map(|index| format!("Person {index}")));
        ParticipantsRepository::add(&pool, "m1", &invited, SOURCE_OUTLOOK_CALENDAR)
            .await
            .unwrap();

        let names = ParticipantsRepository::names(&pool, "m1").await.unwrap();
        // The two blanks are inside the cap window but must not become participants.
        assert_eq!(names.len(), MAX_PARTICIPANTS - 2);
        assert_eq!(names[0], "Person 0");
    }

    #[tokio::test]
    async fn a_meeting_without_participants_reads_as_empty() {
        let pool = pool_with_meeting().await;
        assert!(ParticipantsRepository::names(&pool, "m1")
            .await
            .unwrap()
            .is_empty());
    }
}
