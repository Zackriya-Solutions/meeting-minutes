use crate::database::models::{CalendarAccount, CalendarEvent};
use chrono::{DateTime, Utc};
use sqlx::{Error as SqlxError, SqlitePool};

pub struct CalendarRepository;

impl CalendarRepository {
    /// Upserts the (single, v1) connected Google account.
    pub async fn upsert_account(
        pool: &SqlitePool,
        email: &str,
        access_token: &str,
        refresh_token: &str,
        token_expires_at: DateTime<Utc>,
        scope: &str,
    ) -> Result<(), SqlxError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO calendar_accounts
                (id, provider, email, access_token, refresh_token, token_expires_at, scope, status, connected_at, updated_at)
            VALUES (1, 'google', ?, ?, ?, ?, ?, 'connected', ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                token_expires_at = excluded.token_expires_at,
                scope = excluded.scope,
                status = 'connected',
                updated_at = excluded.updated_at
            "#,
        )
        .bind(email)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expires_at)
        .bind(scope)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn get_account(pool: &SqlitePool) -> Result<Option<CalendarAccount>, SqlxError> {
        sqlx::query_as::<_, CalendarAccount>("SELECT * FROM calendar_accounts WHERE id = 1")
            .fetch_optional(pool)
            .await
    }

    /// Updates just the access token/expiry after a refresh, preserving the refresh token.
    pub async fn update_access_token(
        pool: &SqlitePool,
        access_token: &str,
        token_expires_at: DateTime<Utc>,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE calendar_accounts SET access_token = ?, token_expires_at = ?, status = 'connected', updated_at = ? WHERE id = 1",
        )
        .bind(access_token)
        .bind(token_expires_at)
        .bind(Utc::now())
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn mark_needs_reauth(pool: &SqlitePool) -> Result<(), SqlxError> {
        sqlx::query("UPDATE calendar_accounts SET status = 'needs_reauth', updated_at = ? WHERE id = 1")
            .bind(Utc::now())
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Disconnects the account; synced events cascade-delete with it.
    pub async fn disconnect(pool: &SqlitePool) -> Result<(), SqlxError> {
        sqlx::query("DELETE FROM calendar_accounts WHERE id = 1")
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Upserts a synced event (by Google event id).
    pub async fn upsert_event(pool: &SqlitePool, event: &CalendarEvent) -> Result<(), SqlxError> {
        sqlx::query(
            r#"
            INSERT INTO calendar_events
                (id, calendar_account_id, title, start_time, end_time, meeting_url, meeting_provider, raw_json, synced_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                start_time = excluded.start_time,
                end_time = excluded.end_time,
                meeting_url = excluded.meeting_url,
                meeting_provider = excluded.meeting_provider,
                raw_json = excluded.raw_json,
                synced_at = excluded.synced_at
            "#,
        )
        .bind(&event.id)
        .bind(event.calendar_account_id)
        .bind(&event.title)
        .bind(event.start_time)
        .bind(event.end_time)
        .bind(&event.meeting_url)
        .bind(&event.meeting_provider)
        .bind(&event.raw_json)
        .bind(event.synced_at)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Events starting between `from` and `to`, soonest first.
    pub async fn get_events_in_range(
        pool: &SqlitePool,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>, SqlxError> {
        sqlx::query_as::<_, CalendarEvent>(
            "SELECT * FROM calendar_events WHERE start_time >= ? AND start_time <= ? ORDER BY start_time ASC",
        )
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await
    }
}
