use super::CalendarEvent;
use chrono::{DateTime, Utc};

/// Find the calendar event whose window overlaps `[window_start, window_end]` the most.
/// Used both for post-recording metadata enrichment and (in the auto-detect path) for
/// pre-filling a meeting title before recording starts.
pub fn find_matching_event<'a>(
    events: &'a [CalendarEvent],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Option<&'a CalendarEvent> {
    events
        .iter()
        .filter(|event| event.start_time < window_end && event.end_time > window_start)
        .max_by_key(|event| {
            let overlap_start = event.start_time.max(window_start);
            let overlap_end = event.end_time.min(window_end);
            (overlap_end - overlap_start).num_seconds().max(0)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn event(id: &str, start_hour: u32, end_hour: u32) -> CalendarEvent {
        CalendarEvent {
            id: id.to_string(),
            title: id.to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 1, 1, start_hour, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 1, 1, end_hour, 0, 0).unwrap(),
            meet_link: None,
            attendees: vec![],
        }
    }

    #[test]
    fn picks_the_event_with_greatest_overlap() {
        let events = vec![event("standup", 9, 9), event("planning", 10, 12)];
        // Widen the standup window slightly so it has a non-zero duration to compare against.
        let events = vec![
            CalendarEvent {
                end_time: Utc.with_ymd_and_hms(2026, 1, 1, 9, 30, 0).unwrap(),
                ..events[0].clone()
            },
            events[1].clone(),
        ];

        let window_start = Utc.with_ymd_and_hms(2026, 1, 1, 9, 45, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();

        let matched = find_matching_event(&events, window_start, window_end).unwrap();
        assert_eq!(matched.id, "planning");
    }

    #[test]
    fn returns_none_when_no_event_overlaps() {
        let events = vec![event("standup", 9, 10)];
        let window_start = Utc.with_ymd_and_hms(2026, 1, 1, 14, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 1, 1, 15, 0, 0).unwrap();

        assert!(find_matching_event(&events, window_start, window_end).is_none());
    }
}
