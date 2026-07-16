//! Collections + auto-series suggestion (PLAN.md Phase 5).
//!
//! Series detection — normalized-title similarity + recurring-cadence detection — is
//! pure and unit-tested here. Collection CRUD and applying a suggestion are thin DB ops
//! in the commands layer.

pub mod commands;

use chrono::NaiveDate;
use sqlx::SqlitePool;
use std::collections::HashSet;

/// Minimum meetings in a group before it's worth proposing as a series.
pub const MIN_SERIES_SIZE: usize = 3;
/// Normalized-title similarity above which two meetings are considered "the same series".
pub const TITLE_SIM_THRESHOLD: f64 = 0.90;

/// A meeting reduced to what series detection needs.
#[derive(Debug, Clone)]
pub struct MeetingRef {
    pub id: String,
    pub title: String,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Daily,
    Weekly,
    Biweekly,
    Irregular,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeriesSuggestion {
    pub suggested_name: String,
    pub meeting_ids: Vec<String>,
    pub cadence: Cadence,
}

/// Normalize a title for series grouping: drop digits/punctuation (dates, "#12", etc.),
/// lowercase, collapse whitespace. "Weekly Sync #12 (2026-07-01)" -> "weekly sync".
pub fn normalize_title(title: &str) -> String {
    let lowered = title.to_lowercase();
    if lowered.starts_with("meeting ")
        && (lowered.contains('_')
            || lowered
                .chars()
                .filter(|character| character.is_ascii_digit())
                .count()
                >= 6)
    {
        return String::new();
    }
    let without_machine_prefix = if lowered.starts_with("date-unknown_") {
        lowered.splitn(4, '_').nth(3).unwrap_or("").to_string()
    } else if lowered
        .get(0..10)
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .is_some()
    {
        lowered.splitn(3, '_').nth(2).unwrap_or("").to_string()
    } else {
        lowered
    };
    let canonical = without_machine_prefix
        .replace("мини", "mini")
        .replace("стендап", "standup")
        .replace("планерка", "planning")
        .replace("планёрка", "planning");
    let cleaned: String = canonical
        .chars()
        .map(|c| {
            if c.is_alphabetic() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn derive_series_match_rule<'a>(titles: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let normalized: Vec<String> = titles
        .into_iter()
        .map(normalize_title)
        .filter(|title| !title.is_empty())
        .collect();
    let first = normalized.first()?;
    let mut common: HashSet<&str> = first.split_whitespace().collect();
    for title in normalized.iter().skip(1) {
        let tokens: HashSet<&str> = title.split_whitespace().collect();
        common.retain(|token| tokens.contains(token));
    }
    let rule = first
        .split_whitespace()
        .filter(|token| common.contains(token) && !matches!(*token, "meeting" | "встреча"))
        .collect::<Vec<_>>()
        .join(" ");
    (!rule.is_empty()).then_some(rule)
}

pub fn series_title_matches(rule: &str, title: &str) -> bool {
    let normalized_rule = normalize_title(rule);
    let normalized_title = normalize_title(title);
    if normalized_rule.is_empty()
        || normalized_title.is_empty()
        || normalized_rule
            .split_whitespace()
            .all(|token| matches!(token, "meeting" | "встреча"))
    {
        return false;
    }
    let rule_tokens: HashSet<&str> = normalized_rule.split_whitespace().collect();
    let title_tokens: HashSet<&str> = normalized_title.split_whitespace().collect();
    rule_tokens.iter().all(|token| title_tokens.contains(token))
        || strsim::jaro_winkler(&normalized_rule, &normalized_title) >= TITLE_SIM_THRESHOLD
}

pub async fn auto_assign_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
    title: &str,
) -> Result<Vec<i64>, sqlx::Error> {
    let rules: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.id, c.match_rule FROM collections c
         WHERE c.kind = 'series' AND c.auto_add = 1 AND c.match_rule IS NOT NULL
           AND NOT EXISTS (
             SELECT 1 FROM collection_auto_exclusions e
             WHERE e.collection_id = c.id AND e.meeting_id = ?
           )",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let mut assigned = Vec::new();
    for (collection_id, rule) in rules {
        if series_title_matches(&rule, title) {
            let result = sqlx::query(
                "INSERT OR IGNORE INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)",
            )
            .bind(meeting_id)
            .bind(collection_id)
            .execute(pool)
            .await?;
            if result.rows_affected() > 0 {
                assigned.push(collection_id);
            }
        }
    }
    Ok(assigned)
}

pub async fn auto_assign_unique_template_series(
    pool: &SqlitePool,
    meeting_id: &str,
    template_marker: &str,
) -> Result<Vec<i64>, sqlx::Error> {
    let normalized_marker = normalize_title(template_marker);
    if normalized_marker.is_empty() {
        return Ok(Vec::new());
    }
    let marker_tokens: HashSet<&str> = normalized_marker.split_whitespace().collect();
    let rules: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.id, c.match_rule FROM collections c
         WHERE c.kind = 'series' AND c.auto_add = 1 AND c.match_rule IS NOT NULL
           AND NOT EXISTS (
             SELECT 1 FROM collection_auto_exclusions e
             WHERE e.collection_id = c.id AND e.meeting_id = ?
           )",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let matching: Vec<i64> = rules
        .into_iter()
        .filter_map(|(collection_id, rule)| {
            let normalized_rule = normalize_title(&rule);
            let rule_tokens: HashSet<&str> = normalized_rule.split_whitespace().collect();
            marker_tokens
                .iter()
                .all(|token| rule_tokens.contains(token))
                .then_some(collection_id)
        })
        .collect();

    // A template such as "daily standup" does not identify a particular team.
    // Only make an automatic choice when exactly one enabled series fits.
    let [collection_id] = matching.as_slice() else {
        return Ok(Vec::new());
    };
    let result = sqlx::query(
        "INSERT OR IGNORE INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)",
    )
    .bind(meeting_id)
    .bind(collection_id)
    .execute(pool)
    .await?;
    Ok((result.rows_affected() > 0)
        .then_some(*collection_id)
        .into_iter()
        .collect())
}

/// Classify the cadence of a set of dates by their consecutive day-gaps.
fn detect_cadence(dates: &[NaiveDate]) -> Cadence {
    if dates.len() < 2 {
        return Cadence::Irregular;
    }
    let mut sorted = dates.to_vec();
    sorted.sort();
    let gaps: Vec<i64> = sorted
        .windows(2)
        .map(|w| (w[1] - w[0]).num_days())
        .collect();
    // All gaps must fall in a tolerance band around a target to count as regular.
    let matches = |target: i64, tol: i64| gaps.iter().all(|g| (g - target).abs() <= tol);
    if matches(1, 0) {
        Cadence::Daily
    } else if matches(7, 2) {
        Cadence::Weekly
    } else if matches(14, 3) {
        Cadence::Biweekly
    } else {
        Cadence::Irregular
    }
}

/// Propose series: group meetings by normalized-title similarity, then keep groups that
/// are large enough AND follow a regular cadence.
pub fn suggest_series(meetings: &[MeetingRef], min_group: usize) -> Vec<SeriesSuggestion> {
    // Greedy grouping by normalized-title similarity.
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, m) in meetings.iter().enumerate() {
        let norm = normalize_title(&m.title);
        if norm.is_empty() {
            continue;
        }
        match groups
            .iter_mut()
            .find(|(rep, _)| strsim::jaro_winkler(rep, &norm) >= TITLE_SIM_THRESHOLD)
        {
            Some((_, members)) => members.push(i),
            None => groups.push((norm, vec![i])),
        }
    }

    groups
        .into_iter()
        .filter(|(_, members)| members.len() >= min_group)
        .filter_map(|(rep, members)| {
            let dates: Vec<NaiveDate> = members.iter().map(|&i| meetings[i].date).collect();
            let cadence = detect_cadence(&dates);
            Some(SeriesSuggestion {
                suggested_name: title_case(&rep),
                meeting_ids: members.iter().map(|&i| meetings[i].id.clone()).collect(),
                cadence,
            })
        })
        .collect()
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn m(id: &str, title: &str, date: &str) -> MeetingRef {
        MeetingRef {
            id: id.into(),
            title: title.into(),
            date: d(date),
        }
    }

    #[test]
    fn normalize_strips_dates_and_numbers() {
        assert_eq!(
            normalize_title("Weekly Standup #12 (2026-07-01)"),
            "weekly standup"
        );
        // ё is NOT folded here (title grouping ≠ entity normalization); digits are dropped.
        assert_eq!(normalize_title("Планёрка 3"), "planning");
    }

    #[test]
    fn normalize_strips_import_prefix_and_unifies_standup_synonyms() {
        assert_eq!(
            normalize_title("date-unknown_tue_11-18_standup_mini"),
            "standup mini"
        );
        assert_eq!(
            normalize_title("Мини-стендап команды"),
            "mini standup команды"
        );
        assert!(series_title_matches("standup mini", "Мини-стендап команды"));
        assert_eq!(normalize_title("Meeting 2026-07-16_17-51-16"), "");
        assert!(!series_title_matches(
            "meeting",
            "Meeting 2026-07-16_17-51-16"
        ));
    }

    #[test]
    fn derives_common_rule_from_series_titles() {
        let titles = [
            "date-unknown_mon_11-04_standup_mini",
            "date-unknown_tue_11-18_standup_mini",
            "2025-04-21_14-43_standup_mini",
        ];
        assert_eq!(
            derive_series_match_rule(titles.iter().copied()).as_deref(),
            Some("standup mini")
        );
    }

    #[tokio::test]
    async fn automatic_series_assignment_respects_manual_exclusions() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE collections(
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                auto_add INTEGER NOT NULL,
                match_rule TEXT
            );
            CREATE TABLE meeting_collections(
                meeting_id TEXT NOT NULL,
                collection_id INTEGER NOT NULL,
                PRIMARY KEY(meeting_id, collection_id)
            );
            CREATE TABLE collection_auto_exclusions(
                collection_id INTEGER NOT NULL,
                meeting_id TEXT NOT NULL,
                PRIMARY KEY(collection_id, meeting_id)
            );",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO collections(id, kind, auto_add, match_rule)
             VALUES(1, 'series', 1, 'standup mini')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let first = auto_assign_meeting(&pool, "meeting-1", "Мини-стендап команды")
            .await
            .unwrap();
        let duplicate = auto_assign_meeting(&pool, "meeting-1", "Мини-стендап команды")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO collection_auto_exclusions(collection_id, meeting_id)
             VALUES(1, 'meeting-2')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let excluded = auto_assign_meeting(&pool, "meeting-2", "Мини-стендап команды")
            .await
            .unwrap();

        assert_eq!(first, vec![1]);
        assert!(duplicate.is_empty());
        assert!(excluded.is_empty());

        let unique_template = auto_assign_unique_template_series(&pool, "meeting-3", "standup")
            .await
            .unwrap();
        assert_eq!(unique_template, vec![1]);

        sqlx::query(
            "INSERT INTO collections(id, kind, auto_add, match_rule)
             VALUES(2, 'series', 1, 'standup productivity')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let ambiguous_template = auto_assign_unique_template_series(&pool, "meeting-4", "standup")
            .await
            .unwrap();
        assert!(ambiguous_template.is_empty());
    }

    #[test]
    fn weekly_standups_are_suggested() {
        let meetings = vec![
            m("a", "Weekly Standup #1", "2026-06-01"),
            m("b", "Weekly Standup #2", "2026-06-08"),
            m("c", "Weekly Standup #3", "2026-06-15"),
            m("d", "Weekly Standup #4", "2026-06-22"),
        ];
        let s = suggest_series(&meetings, MIN_SERIES_SIZE);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].cadence, Cadence::Weekly);
        assert_eq!(s[0].meeting_ids.len(), 4);
        assert_eq!(s[0].suggested_name, "Weekly Standup");
    }

    #[test]
    fn unrelated_meetings_yield_no_series() {
        let meetings = vec![
            m("a", "Kickoff Alpha", "2026-06-01"),
            m("b", "Budget Review", "2026-06-09"),
            m("c", "Random Chat", "2026-07-02"),
        ];
        assert!(suggest_series(&meetings, MIN_SERIES_SIZE).is_empty());
    }

    #[test]
    fn same_title_with_irregular_dates_is_still_suggested() {
        let meetings = vec![
            m("a", "Sync", "2026-06-01"),
            m("b", "Sync", "2026-06-03"),
            m("c", "Sync", "2026-07-20"),
        ];
        let suggestions = suggest_series(&meetings, MIN_SERIES_SIZE);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].cadence, Cadence::Irregular);
    }
}
