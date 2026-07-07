//! Collections + auto-series suggestion (PLAN.md Phase 5).
//!
//! Series detection — normalized-title similarity + recurring-cadence detection — is
//! pure and unit-tested here. Collection CRUD and applying a suggestion are thin DB ops
//! in the commands layer.

pub mod commands;

use chrono::NaiveDate;

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
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphabetic() || c.is_whitespace() { c } else { ' ' })
        .collect();
    cleaned.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Classify the cadence of a set of dates by their consecutive day-gaps.
fn detect_cadence(dates: &[NaiveDate]) -> Cadence {
    if dates.len() < 2 {
        return Cadence::Irregular;
    }
    let mut sorted = dates.to_vec();
    sorted.sort();
    let gaps: Vec<i64> = sorted.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
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
            if cadence == Cadence::Irregular {
                return None;
            }
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

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn m(id: &str, title: &str, date: &str) -> MeetingRef {
        MeetingRef { id: id.into(), title: title.into(), date: d(date) }
    }

    #[test]
    fn normalize_strips_dates_and_numbers() {
        assert_eq!(normalize_title("Weekly Standup #12 (2026-07-01)"), "weekly standup");
        // ё is NOT folded here (title grouping ≠ entity normalization); digits are dropped.
        assert_eq!(normalize_title("Планёрка 3"), "планёрка");
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
    fn same_title_but_irregular_dates_not_suggested() {
        let meetings = vec![
            m("a", "Sync", "2026-06-01"),
            m("b", "Sync", "2026-06-03"),
            m("c", "Sync", "2026-07-20"),
        ];
        // Same title, but gaps (2, 47 days) are irregular -> no suggestion.
        assert!(suggest_series(&meetings, MIN_SERIES_SIZE).is_empty());
    }
}
