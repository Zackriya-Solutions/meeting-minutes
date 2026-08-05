//! The report sections the meeting screen re-uses: the score with its verdict,
//! «Что мешало», «Покрытие повестки», «Числа встречи», «Динамика встречи» and the
//! «Лента встречи» timeline.
//!
//! The HTML report renders these from the live stage artifacts; the app reads them back
//! from the `artifacts` JSON snapshot persisted with the completed run, so opening a
//! meeting costs one row read and no LLM work. Every stage is independently optional —
//! a stage that failed during the run is simply absent here, exactly as it renders as a
//! placeholder in the HTML.
//!
//! Provenance ("момент" links) is recomputed from the CURRENT transcript with the same
//! deterministic timeline the pipeline used ([`crate::report::dynamics::timeline`]).
//! Because `seg` values are positional, a transcript that was re-split after the report
//! was built would silently point them at other moments; `segment_count` in the snapshot
//! guards against that and drops the links instead.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{from_value, Value};

use crate::report::dynamics::{Dynamics, TimedSegment};
use crate::report::prompts::{
    Commitments, Decisions, DisagreementsConcepts, Insights, Numbers, Roles, Topics,
};
use crate::report::render::Score;

/// Longest text a timeline tooltip carries; the transcript tab has the full reply.
const TIMELINE_PREVIEW_CHARS: usize = 90;

/// The meeting's current transcript, placed on the pipeline's deterministic timeline. The
/// «Лента встречи» blocks and every moment link are built from this, not from the snapshot,
/// so they address the transcript the user is actually looking at.
pub struct TranscriptTimeline<'a> {
    pub timed: &'a [TimedSegment],
    pub texts: &'a [String],
    /// Recomputed over the same segments — supplies the speaker lanes and their order.
    pub dynamics: &'a Dynamics,
}

/// Everything the meeting screen needs from one completed report.
#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsSections {
    pub report_id: String,
    /// Raw SQLite timestamp of the run that produced these sections (UTC, `YYYY-MM-DD HH:MM:SS`).
    pub completed_at: Option<String>,
    pub score: Option<ScoreSection>,
    /// Bullets from the synthesis stage; empty when that stage failed or found nothing.
    pub what_hindered: Vec<String>,
    pub agenda: Vec<AgendaRow>,
    pub numbers: Vec<NumberRow>,
    pub dynamics: Option<DynamicsSection>,
    pub roles: Vec<RoleRow>,
    /// `None` when the transcript could not be read — the tab then has nothing to draw.
    pub timeline: Option<TimelineSection>,
}

/// «Лента встречи»: topic bands, event markers, and one speech lane per speaker.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineSection {
    pub duration_secs: f64,
    /// Speaker lanes, most talkative first; a turn's `lane` indexes into this.
    pub lanes: Vec<TimelineLane>,
    pub turns: Vec<TimelineTurn>,
    pub topics: Vec<TimelineTopic>,
    pub markers: Vec<TimelineMarker>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineLane {
    pub label: String,
    /// Colour slot 0..=3; 4+ share the muted slot, as in the HTML report.
    pub palette_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineTurn {
    pub start: f64,
    pub end: f64,
    pub lane: usize,
    /// First ~90 characters of the reply, for the hover tooltip.
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineTopic {
    pub start: f64,
    pub end: f64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineMarker {
    pub at_seconds: f64,
    /// "decision" | "disagreement" | "commitment"
    pub kind: String,
    pub text: String,
}

/// The deterministic score plus the LLM one-liner the report shows next to it. Component
/// percentages are pre-rounded to whole percents, matching the rendered HTML meters.
#[derive(Debug, Clone, Serialize)]
pub struct ScoreSection {
    pub total: i64,
    /// Synthesis verdict; empty string when the insights stage produced none.
    pub verdict: String,
    pub coverage_pct: i64,
    pub owners_pct: i64,
    pub deadline_pct: i64,
    pub dod_pct: i64,
    pub qa_pct: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgendaRow {
    pub item: String,
    /// "covered" | "partial" | "missed"
    pub status: String,
    pub at_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumberRow {
    pub metric: String,
    pub value: String,
    /// Reviewer note on the figure; empty when the stage had nothing to add.
    pub check: String,
    /// "ok" | "warn" | "info"
    pub status: String,
    pub at_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DynamicsSection {
    pub duration_secs: f64,
    /// Fraction of wall-clock time that was speech (0..1).
    pub speech_density: f64,
    pub turn_count: i64,
    pub total_questions: i64,
    pub pauses_over_3s: i64,
    pub pauses_over_10s: i64,
    /// `None` when the stage failed — the tile shows "—" rather than a wrong zero.
    pub decisions_count: Option<i64>,
    pub commitments_count: Option<i64>,
    pub speakers: Vec<SpeakerRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerRow {
    pub label: String,
    pub talk_secs: f64,
    /// Share of total speech time (0..1).
    pub talk_share: f64,
    pub questions: i64,
    pub turns: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleRow {
    pub speaker: String,
    pub role: String,
    pub evidence: String,
    pub at_seconds: Option<f64>,
}

/// Rebuild the UI sections from a persisted artifacts snapshot. `transcript` carries the
/// meeting's current transcript on the deterministic timeline; it resolves `seg` indices
/// into moment links and supplies the «Лента встречи» lanes. `None` (transcript unreadable)
/// still yields every text section, just without times or a feed.
pub fn build(
    report_id: &str,
    completed_at: Option<String>,
    artifacts_json: &str,
    transcript: Option<&TranscriptTimeline>,
) -> Result<AnalyticsSections, String> {
    let root: Value = serde_json::from_str(artifacts_json)
        .map_err(|e| format!("Снимок отчёта повреждён: {e}"))?;

    let seg_times: Vec<f64> = transcript
        .map(|tr| tr.timed.iter().map(|t| t.start).collect())
        .unwrap_or_default();

    // Written since this feature landed; absent in older rows, which we trust as-is.
    let provenance_ok = match root.get("segment_count").and_then(Value::as_u64) {
        Some(n) => n as usize == seg_times.len(),
        None => true,
    };
    let at = |seg: i64| -> Option<f64> {
        if !provenance_ok || seg < 0 {
            return None;
        }
        seg_times.get(seg as usize).copied()
    };

    let stage = |key: &str| -> Option<Value> {
        root.get(key).filter(|value| !value.is_null()).cloned()
    };
    let stage_len = |key: &str, field: &str| -> Option<i64> {
        Some(root.get(key)?.get(field)?.as_array()?.len() as i64)
    };

    let insights: Option<Insights> = stage("insights").and_then(|v| from_value(v).ok());
    let topics: Option<Topics> = stage("topics").and_then(|v| from_value(v).ok());
    let numbers: Option<Numbers> = stage("numbers").and_then(|v| from_value(v).ok());
    let roles: Option<Roles> = stage("roles").and_then(|v| from_value(v).ok());
    let dynamics: Option<Dynamics> = stage("dynamics").and_then(|v| from_value(v).ok());
    let score: Option<Score> = stage("score").and_then(|v| from_value(v).ok());

    let verdict = insights
        .as_ref()
        .map(|i| i.verdict.trim().to_string())
        .unwrap_or_default();

    let timeline = transcript.map(|tr| build_timeline(tr, topics.as_ref(), &root, &at));

    Ok(AnalyticsSections {
        report_id: report_id.to_string(),
        completed_at,
        score: score.map(|s| ScoreSection {
            total: s.total,
            verdict,
            coverage_pct: s.coverage_pct.round() as i64,
            owners_pct: s.owners_pct.round() as i64,
            deadline_pct: s.deadline_pct.round() as i64,
            dod_pct: s.dod_pct.round() as i64,
            qa_pct: s.qa_pct.round() as i64,
        }),
        what_hindered: insights
            .as_ref()
            .map(|i| {
                i.what_hindered
                    .iter()
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        agenda: topics
            .as_ref()
            .map(|t| {
                t.agenda
                    .iter()
                    .map(|a| AgendaRow {
                        item: a.item.clone(),
                        status: a.status.clone(),
                        at_seconds: at(a.seg),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        numbers: numbers
            .map(|n| {
                n.numbers
                    .into_iter()
                    .map(|num| NumberRow {
                        metric: num.metric,
                        value: num.value,
                        check: num.check,
                        status: num.status,
                        at_seconds: at(num.seg),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        dynamics: dynamics.map(|d| DynamicsSection {
            duration_secs: d.duration_secs,
            speech_density: d.speech_density,
            turn_count: d.turn_count as i64,
            total_questions: d.total_questions as i64,
            pauses_over_3s: d.pauses_over_3s as i64,
            pauses_over_10s: d.pauses_over_10s as i64,
            decisions_count: stage_len("decisions", "decisions"),
            commitments_count: stage_len("commitments", "commitments"),
            speakers: d
                .speakers
                .into_iter()
                .map(|s| SpeakerRow {
                    label: s.label,
                    talk_secs: s.talk_secs,
                    talk_share: s.talk_share,
                    questions: s.questions as i64,
                    turns: s.turns as i64,
                })
                .collect(),
        }),
        roles: roles
            .map(|r| {
                r.roles
                    .into_iter()
                    .map(|role| RoleRow {
                        at_seconds: at(role.seg),
                        speaker: role.speaker,
                        role: role.role,
                        evidence: role.evidence,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        timeline,
    })
}

fn preview(text: &str, chars: usize) -> String {
    text.trim().chars().take(chars).collect()
}

/// Assemble the feed: lanes and speech blocks from the current transcript, topic bands and
/// event markers from the snapshot. Markers whose moment could not be resolved are dropped
/// rather than pinned to second 0.
fn build_timeline(
    transcript: &TranscriptTimeline,
    topics: Option<&Topics>,
    root: &Value,
    at: &dyn Fn(i64) -> Option<f64>,
) -> TimelineSection {
    let dynamics = transcript.dynamics;
    let duration = dynamics.duration_secs.max(0.0);

    let lane_of: HashMap<&str, usize> = dynamics
        .speakers
        .iter()
        .enumerate()
        .map(|(index, s)| (s.key.as_str(), index))
        .collect();

    let turns: Vec<TimelineTurn> = transcript
        .timed
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let lane = *lane_of.get(t.speaker_key.as_str())?;
            Some(TimelineTurn {
                start: t.start,
                end: t.end.max(t.start),
                lane,
                text: preview(
                    transcript.texts.get(i).map(String::as_str).unwrap_or(""),
                    TIMELINE_PREVIEW_CHARS,
                ),
            })
        })
        .collect();

    let topic_bands: Vec<TimelineTopic> = topics
        .map(|t| {
            t.topics
                .iter()
                .filter(|tp| !tp.name.trim().is_empty())
                .map(|tp| {
                    let start = (tp.start_s as f64).clamp(0.0, duration);
                    TimelineTopic {
                        start,
                        end: (tp.end_s as f64).clamp(start, duration.max(start)),
                        name: tp.name.trim().to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let mut markers: Vec<TimelineMarker> = Vec::new();
    let stage = |key: &str| -> Option<Value> {
        root.get(key).filter(|value| !value.is_null()).cloned()
    };
    if let Some(d) = stage("decisions").and_then(|v| from_value::<Decisions>(v).ok()) {
        for dec in &d.decisions {
            if let Some(t) = at(dec.seg) {
                markers.push(TimelineMarker {
                    at_seconds: t,
                    kind: "decision".to_string(),
                    text: preview(&dec.statement, TIMELINE_PREVIEW_CHARS),
                });
            }
        }
    }
    if let Some(dc) = stage("disagreements_concepts")
        .and_then(|v| from_value::<DisagreementsConcepts>(v).ok())
    {
        for dis in &dc.disagreements {
            let seg = dis.positions.first().map(|p| p.seg);
            if let Some(t) = seg.and_then(at) {
                markers.push(TimelineMarker {
                    at_seconds: t,
                    kind: "disagreement".to_string(),
                    text: preview(&dis.topic, TIMELINE_PREVIEW_CHARS),
                });
            }
        }
    }
    if let Some(c) = stage("commitments").and_then(|v| from_value::<Commitments>(v).ok()) {
        for cm in &c.commitments {
            if let Some(t) = at(cm.seg) {
                markers.push(TimelineMarker {
                    at_seconds: t,
                    kind: "commitment".to_string(),
                    text: format!("{} — {}", preview(&cm.who, 24), preview(&cm.what, 60)),
                });
            }
        }
    }
    markers.sort_by(|a, b| a.at_seconds.total_cmp(&b.at_seconds));

    TimelineSection {
        duration_secs: duration,
        lanes: dynamics
            .speakers
            .iter()
            .map(|s| TimelineLane {
                label: s.label.clone(),
                palette_index: s.palette_index,
            })
            .collect(),
        turns,
        topics: topic_bands,
        markers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::dynamics::SpeakerDyn;

    /// Three replies from two speakers on the pipeline's timeline.
    fn timed() -> Vec<TimedSegment> {
        vec![
            TimedSegment { start: 0.0, end: 8.0, speaker_key: "id:1".into() },
            TimedSegment { start: 12.5, end: 20.0, speaker_key: "id:2".into() },
            TimedSegment { start: 61.0, end: 65.0, speaker_key: "id:1".into() },
        ]
    }

    fn texts() -> Vec<String> {
        vec!["Начнём".into(), "Вопрос по срокам?".into(), "Решили: релиз в пятницу".into()]
    }

    fn speaker(key: &str, label: &str, index: usize, talk: f64) -> SpeakerDyn {
        SpeakerDyn {
            key: key.into(),
            label: label.into(),
            talk_secs: talk,
            talk_share: 0.5,
            questions: 1,
            turns: 1,
            palette_index: index,
        }
    }

    fn live_dynamics() -> Dynamics {
        Dynamics {
            duration_secs: 65.0,
            speech_density: 0.5,
            turn_count: 3,
            total_questions: 1,
            pauses_over_3s: 1,
            pauses_over_10s: 1,
            speakers: vec![speaker("id:1", "Аня", 0, 12.0), speaker("id:2", "Иван", 1, 7.5)],
        }
    }

    fn snapshot() -> String {
        serde_json::json!({
            "segment_count": 3,
            "dynamics": {
                "duration_secs": 61.0,
                "speech_density": 0.5,
                "turn_count": 3,
                "total_questions": 2,
                "pauses_over_3s": 1,
                "pauses_over_10s": 0,
                "speakers": [{
                    "key": "id:1", "label": "Аня", "talk_secs": 40.0, "talk_share": 0.8,
                    "questions": 2, "turns": 2, "palette_index": 0
                }]
            },
            "score": {
                "total": 70, "coverage_pct": 75.0, "owners_pct": 50.4,
                "deadline_pct": 50.0, "dod_pct": 100.0, "qa_pct": 76.0
            },
            "topics": {
                "topics": [
                    { "name": "Сроки", "start_s": 0.0, "end_s": 30.0 },
                    { "name": "Найм", "start_s": 30.0, "end_s": 900.0 }
                ],
                "agenda": [
                    { "item": "Сроки релиза", "status": "covered", "seg": 1 },
                    { "item": "Найм", "status": "missed", "seg": 99 }
                ]
            },
            "disagreements_concepts": { "disagreements": [
                { "topic": "Сроки", "positions": [{ "who": "Иван", "stance": "против", "seg": 1 }] }
            ]},
            "decisions": { "decisions": [{ "statement": "Релиз в пятницу", "seg": 2 }] },
            "commitments": { "commitments": [] },
            "numbers": { "numbers": [
                { "metric": "Конверсия", "value": "12%", "seg": 2, "check": "сходится", "status": "ok" }
            ]},
            "roles": { "roles": [
                { "speaker": "Аня", "role": "ведущий", "evidence": "держала повестку", "seg": 0 }
            ]},
            "insights": {
                "insights": [],
                "verdict": "Встреча закрыла сроки, но не найм.",
                "what_hindered": ["Нет владельца у половины задач", "  "]
            }
        })
        .to_string()
    }

    #[test]
    fn build_maps_every_section_and_resolves_provenance() {
        let (timed, texts, live) = (timed(), texts(), live_dynamics());
        let transcript = TranscriptTimeline { timed: &timed, texts: &texts, dynamics: &live };
        let s = build(
            "r1",
            Some("2026-08-05 10:00:00".into()),
            &snapshot(),
            Some(&transcript),
        )
        .unwrap();

        let score = s.score.expect("score present");
        assert_eq!(score.total, 70);
        assert_eq!(score.verdict, "Встреча закрыла сроки, но не найм.");
        // Component percentages arrive rounded, like the HTML meters.
        assert_eq!(score.owners_pct, 50);

        // Blank bullets are dropped rather than rendered as empty rows.
        assert_eq!(s.what_hindered, vec!["Нет владельца у половины задач"]);

        assert_eq!(s.agenda.len(), 2);
        assert_eq!(s.agenda[0].at_seconds, Some(12.5));
        // An out-of-range seg yields no link instead of a wrong one.
        assert_eq!(s.agenda[1].at_seconds, None);

        assert_eq!(s.numbers[0].value, "12%");
        assert_eq!(s.numbers[0].at_seconds, Some(61.0));
        assert_eq!(s.roles[0].at_seconds, Some(0.0));

        let dyn_section = s.dynamics.expect("dynamics present");
        assert_eq!(dyn_section.decisions_count, Some(1));
        assert_eq!(dyn_section.commitments_count, Some(0));
        assert_eq!(dyn_section.speakers[0].label, "Аня");
    }

    #[test]
    fn failed_stages_are_absent_not_zeroed() {
        let json = serde_json::json!({
            "segment_count": 3,
            "dynamics": {
                "duration_secs": 61.0, "speech_density": 0.5, "turn_count": 3,
                "total_questions": 2, "pauses_over_3s": 1, "pauses_over_10s": 0,
                "speakers": []
            },
            "score": null, "topics": null, "numbers": null, "roles": null, "insights": null,
            "decisions": null, "commitments": null
        })
        .to_string();

        let (timed, texts, live) = (timed(), texts(), live_dynamics());
        let transcript = TranscriptTimeline { timed: &timed, texts: &texts, dynamics: &live };
        let s = build("r1", None, &json, Some(&transcript)).unwrap();
        assert!(s.score.is_none());
        assert!(s.what_hindered.is_empty());
        assert!(s.agenda.is_empty());
        assert!(s.numbers.is_empty());
        let dyn_section = s.dynamics.expect("dynamics is local and always present");
        assert_eq!(dyn_section.decisions_count, None);
        assert_eq!(dyn_section.commitments_count, None);
    }

    /// A transcript re-split after the report was built renumbers segments, so the stored
    /// indices must not be resolved against the new timeline.
    #[test]
    fn provenance_dropped_when_transcript_length_changed() {
        let (mut timed, mut texts, live) = (timed(), texts(), live_dynamics());
        timed.pop();
        texts.pop();
        let transcript = TranscriptTimeline { timed: &timed, texts: &texts, dynamics: &live };
        let s = build("r1", None, &snapshot(), Some(&transcript)).unwrap();
        assert!(s.agenda.iter().all(|a| a.at_seconds.is_none()));
        assert!(s.numbers.iter().all(|n| n.at_seconds.is_none()));
        assert!(s.roles.iter().all(|r| r.at_seconds.is_none()));
        // The sections themselves still render — only the links are withheld.
        assert_eq!(s.agenda.len(), 2);
    }

    #[test]
    fn corrupt_snapshot_is_an_error_not_a_panic() {
        assert!(build("r1", None, "{not json", None).is_err());
    }

    /// The feed's lanes and blocks come from the CURRENT transcript; its bands and markers
    /// come from the snapshot.
    #[test]
    fn timeline_lanes_blocks_bands_and_markers() {
        let (timed, texts, live) = (timed(), texts(), live_dynamics());
        let transcript = TranscriptTimeline { timed: &timed, texts: &texts, dynamics: &live };
        let feed = build("r1", None, &snapshot(), Some(&transcript))
            .unwrap()
            .timeline
            .expect("timeline present when a transcript is available");

        assert_eq!(feed.duration_secs, 65.0);
        assert_eq!(
            feed.lanes.iter().map(|l| l.label.as_str()).collect::<Vec<_>>(),
            vec!["Аня", "Иван"]
        );
        // One block per reply, on its speaker's lane, carrying a tooltip preview.
        assert_eq!(feed.turns.len(), 3);
        assert_eq!(feed.turns[1].lane, 1);
        assert_eq!(feed.turns[1].text, "Вопрос по срокам?");

        // Bands are clamped to the meeting's own length.
        assert_eq!(feed.topics.len(), 2);
        assert_eq!(feed.topics[1].end, 65.0);

        // A decision (seg 2 -> 61s) and a disagreement (seg 1 -> 12.5s), in time order.
        assert_eq!(
            feed.markers.iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(),
            vec!["disagreement", "decision"]
        );
        assert_eq!(feed.markers[1].at_seconds, 61.0);
    }

    /// Without a readable transcript there is nothing to lay a feed on, but the text
    /// sections still render.
    #[test]
    fn timeline_absent_without_transcript() {
        let s = build("r1", None, &snapshot(), None).unwrap();
        assert!(s.timeline.is_none());
        assert_eq!(s.agenda.len(), 2);
        assert!(s.agenda.iter().all(|a| a.at_seconds.is_none()));
    }

    /// Reports written before `segment_count` existed keep their links.
    #[test]
    fn legacy_snapshot_without_segment_count_keeps_provenance() {
        let json = snapshot().replace("\"segment_count\":3,", "");
        let (mut timed, mut texts, live) = (timed(), texts(), live_dynamics());
        timed.pop();
        texts.pop();
        let transcript = TranscriptTimeline { timed: &timed, texts: &texts, dynamics: &live };
        let s = build("r1", None, &json, Some(&transcript)).unwrap();
        assert_eq!(s.agenda[0].at_seconds, Some(12.5));
    }
}
