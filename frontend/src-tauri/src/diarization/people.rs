//! Voice matching: recognises people the user named in other meetings by their stored voices.

use crate::database::repositories::person::PeopleRepository;
use crate::database::repositories::speaker::{
    blob_to_embedding, NameSource, NewSpeaker, SpeakersRepository, SuggestionSource, CLEAR_NAME,
};
use crate::diarization::assign::greedy_pairs;
use crate::diarization::cluster::cosine;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Score at or above which a voice is linked to a person automatically (shown as "auto").
/// Measured with `calibrate_voice_thresholds` on four recorded meetings: the same person scored
/// 0.82–0.91 across meetings, different people in one meeting at most 0.51. Both thresholds sit
/// inside that gap with room on either side.
pub const VOICE_STRONG: f32 = 0.75;
/// Score at or above which a person is suggested for a voice.
pub const VOICE_WEAK: f32 = 0.60;

/// A meeting speaker as voice matching sees it.
#[derive(Debug, Clone)]
pub struct VoiceSpeaker {
    pub key: String,
    /// None for hand-made speakers (rows reassigned to a new speaker).
    pub embedding: Option<Vec<f32>>,
    pub display_name: Option<String>,
    pub person_id: Option<String>,
}

/// A person and the voices that teach it: centroids of speakers the user named.
#[derive(Debug, Clone)]
pub struct PersonVoice {
    pub person_id: String,
    pub name: String,
    pub exemplars: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStrength {
    Strong,
    Weak,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceMatch {
    pub key: String,
    pub person_id: String,
    pub name: String,
    pub score: f32,
    pub strength: MatchStrength,
}

/// Best cosine between a speaker's centroid and a person's exemplars; 0 without exemplars.
pub fn person_score(centroid: &[f32], exemplars: &[Vec<f32>]) -> f32 {
    exemplars.iter().map(|e| cosine(centroid, e)).fold(0.0, f32::max)
}

/// Suggestion reason shown to the user.
pub fn voice_reason(score: f32) -> String {
    format!("voice match {score:.2}")
}

/// Pairs ≥ weak, best first, one person per speaker and one speaker per person. Skips speakers
/// without an embedding, with a display name or a person; people already linked in the meeting;
/// rejected pairs.
pub fn assign_voices(
    speakers: &[VoiceSpeaker],
    people: &[PersonVoice],
    rejected: &HashSet<(String, String)>,
    strong: f32,
    weak: f32,
) -> Vec<VoiceMatch> {
    let linked: HashSet<&str> = speakers.iter().filter_map(|s| s.person_id.as_deref()).collect();
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (i, s) in speakers.iter().enumerate() {
        let Some(embedding) = s.embedding.as_deref().filter(|e| !e.is_empty()) else { continue };
        if s.display_name.is_some() || s.person_id.is_some() {
            continue;
        }
        for (j, p) in people.iter().enumerate() {
            if linked.contains(p.person_id.as_str()) || rejected.contains(&(s.key.clone(), p.person_id.clone())) {
                continue;
            }
            let score = person_score(embedding, &p.exemplars);
            if score >= weak {
                pairs.push((score, i, j));
            }
        }
    }
    greedy_pairs(pairs, speakers.len(), people.len())
        .into_iter()
        .map(|(score, i, j)| VoiceMatch {
            key: speakers[i].key.clone(),
            person_id: people[j].person_id.clone(),
            name: people[j].name.clone(),
            score,
            strength: if score >= strong { MatchStrength::Strong } else { MatchStrength::Weak },
        })
        .collect()
}

/// Exemplars of every person: speakers the user named (or confirmed) that are linked to the
/// person and have a stored voice. Automatic names never teach a voice. `exclude_meeting` leaves
/// out the meeting whose speakers are being replaced.
pub async fn exemplars_conn(conn: &mut SqliteConnection, exclude_meeting: Option<&str>) -> Result<Vec<PersonVoice>, SqlxError> {
    exemplars_of_conn(conn, exclude_meeting, None).await
}

/// `exemplars_conn`, limited to one person when `person_id` is set.
async fn exemplars_of_conn(
    conn: &mut SqliteConnection,
    exclude_meeting: Option<&str>,
    person_id: Option<&str>,
) -> Result<Vec<PersonVoice>, SqlxError> {
    let rows: Vec<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT p.id, p.name, ms.embedding
         FROM meeting_speakers ms
         JOIN people p ON p.id = ms.person_id
         WHERE ms.name_source = 'user' AND ms.embedding IS NOT NULL AND (?1 IS NULL OR ms.meeting_id <> ?1)
           AND (?2 IS NULL OR ms.person_id = ?2)
         ORDER BY p.id",
    )
    .bind(exclude_meeting)
    .bind(person_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut people: Vec<PersonVoice> = Vec::new();
    for (person_id, name, blob) in rows {
        let exemplar = blob_to_embedding(&blob);
        if exemplar.is_empty() {
            continue;
        }
        match people.last_mut() {
            Some(p) if p.person_id == person_id => p.exemplars.push(exemplar),
            _ => people.push(PersonVoice { person_id, name, exemplars: vec![exemplar] }),
        }
    }
    Ok(people)
}

/// Match a speaker write against the people named in other meetings: a strong match links the
/// speaker (name shown as auto), a weak one becomes a voice suggestion unless the speaker already
/// has a suggestion. Named speakers, rejected pairs and people already in the meeting are left
/// alone. Does nothing with `remember_voices` off. Returns (linked, suggested).
pub async fn match_new_speakers_conn(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    speakers: &mut [NewSpeaker],
    remember_voices: bool,
) -> Result<(usize, usize), SqlxError> {
    if !remember_voices || speakers.is_empty() {
        return Ok((0, 0));
    }
    let people = exemplars_conn(&mut *conn, Some(meeting_id)).await?;
    if people.is_empty() {
        return Ok((0, 0));
    }
    let rejected = PeopleRepository::rejections_conn(&mut *conn, meeting_id).await?;
    let voices: Vec<VoiceSpeaker> = speakers
        .iter()
        .map(|s| VoiceSpeaker {
            key: s.key.clone(),
            embedding: Some(s.embedding.clone()),
            display_name: s.display_name.clone(),
            person_id: s.link.person_id.clone(),
        })
        .collect();
    let (mut linked, mut suggested) = (0, 0);
    for m in assign_voices(&voices, &people, &rejected, VOICE_STRONG, VOICE_WEAK) {
        let Some(s) = speakers.iter_mut().find(|s| s.key == m.key) else { continue };
        match m.strength {
            MatchStrength::Strong => {
                s.display_name = Some(m.name);
                s.link.person_id = Some(m.person_id);
                s.link.name_source = Some(NameSource::Voice);
                s.link.clear_suggestion();
                linked += 1;
            }
            MatchStrength::Weak if !s.link.has_suggestion() => {
                s.link.suggested_person_id = Some(m.person_id);
                s.link.suggested_name = Some(m.name);
                s.link.suggestion_source = Some(SuggestionSource::Voice);
                s.link.suggestion_reason = Some(voice_reason(m.score));
                suggested += 1;
            }
            MatchStrength::Weak => {}
        }
    }
    Ok((linked, suggested))
}

/// A speaker of another meeting named after a person by propagation; the unit Undo reverts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PropagatedLink {
    pub meeting_id: String,
    pub speaker_key: String,
    pub person_id: String,
}

/// After a speaker is named or confirmed: link the person's voice, at VOICE_STRONG or above only,
/// to unnamed speakers of other meetings. Skips meetings `is_busy` reports (a running job would
/// overwrite them), rejected pairs and meetings where the person is already linked. Only the name
/// and the link are written; a suggestion the speaker has stays unless it names the person.
/// Returns the links made, in meeting order.
pub async fn propagate_person(
    pool: &SqlitePool,
    person_id: &str,
    is_busy: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Vec<PropagatedLink>, SqlxError> {
    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let Some(voice) = exemplars_of_conn(&mut tx, None, Some(person_id)).await?.pop() else {
        return Ok(Vec::new());
    };
    // Unnamed voiced speakers of meetings where the person is not linked yet.
    let candidates: Vec<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT ms.meeting_id, ms.speaker_key, ms.embedding FROM meeting_speakers ms
         WHERE ms.display_name IS NULL AND ms.person_id IS NULL AND ms.embedding IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM meeting_speakers o WHERE o.meeting_id = ms.meeting_id AND o.person_id = ?)
         ORDER BY ms.meeting_id, ms.speaker_key",
    )
    .bind(person_id)
    .fetch_all(&mut *tx)
    .await?;
    let rejected: HashSet<(String, String)> =
        sqlx::query_as::<_, (String, String)>("SELECT meeting_id, speaker_key FROM speaker_rejections WHERE person_id = ?")
            .bind(person_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let mut meetings: BTreeMap<String, Vec<VoiceSpeaker>> = BTreeMap::new();
    for (meeting_id, key, blob) in candidates {
        if rejected.contains(&(meeting_id.clone(), key.clone())) {
            continue;
        }
        let speaker = VoiceSpeaker { key, embedding: Some(blob_to_embedding(&blob)), display_name: None, person_id: None };
        meetings.entry(meeting_id).or_default().push(speaker);
    }
    let mut links = Vec::new();
    for (meeting_id, speakers) in meetings {
        if is_busy(meeting_id.as_str()) {
            continue;
        }
        for m in assign_voices(&speakers, std::slice::from_ref(&voice), &HashSet::new(), VOICE_STRONG, VOICE_STRONG) {
            let result = sqlx::query(
                "UPDATE meeting_speakers SET display_name = ?, person_id = ?, name_source = 'voice'
                 WHERE meeting_id = ? AND speaker_key = ? AND display_name IS NULL",
            )
            .bind(&voice.name)
            .bind(person_id)
            .bind(&meeting_id)
            .bind(&m.key)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 1 {
                SpeakersRepository::settle_links_conn(&mut tx, &meeting_id).await?;
                links.push(PropagatedLink { meeting_id: meeting_id.clone(), speaker_key: m.key, person_id: person_id.to_string() });
            }
        }
    }
    tx.commit().await?;
    Ok(links)
}

/// Undo a propagation: unlink exactly the listed speakers that are still voice links to that
/// person (one the user confirmed or renamed since is kept). Suggestions are untouched and no
/// rejection is recorded, so a later strong match may link the person again. Returns the
/// meetings that changed.
pub async fn undo_propagation(pool: &SqlitePool, links: &[PropagatedLink]) -> Result<Vec<String>, SqlxError> {
    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let mut changed = BTreeSet::new();
    for l in links {
        let result = sqlx::query(&format!(
            "UPDATE meeting_speakers SET {CLEAR_NAME}
             WHERE meeting_id = ? AND speaker_key = ? AND person_id = ? AND name_source = 'voice'"
        ))
        .bind(&l.meeting_id)
        .bind(&l.speaker_key)
        .bind(&l.person_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() > 0 {
            changed.insert(l.meeting_id.clone());
        }
    }
    tx.commit().await?;
    Ok(changed.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speaker(key: &str, embedding: &[f32]) -> VoiceSpeaker {
        VoiceSpeaker { key: key.into(), embedding: Some(embedding.to_vec()), display_name: None, person_id: None }
    }

    fn person(id: &str, exemplars: &[&[f32]]) -> PersonVoice {
        PersonVoice { person_id: id.into(), name: id.to_uppercase(), exemplars: exemplars.iter().map(|e| e.to_vec()).collect() }
    }

    fn none_rejected() -> HashSet<(String, String)> {
        HashSet::new()
    }

    fn pairs(matches: &[VoiceMatch]) -> Vec<(&str, &str, MatchStrength)> {
        matches.iter().map(|m| (m.key.as_str(), m.person_id.as_str(), m.strength)).collect()
    }

    #[test]
    fn score_is_max_over_exemplars() {
        let score = person_score(&[1.0, 0.0], &[vec![0.0, 1.0], vec![0.8, 0.6]]);
        assert!((score - 0.8).abs() < 1e-6, "{score}");
        assert_eq!(person_score(&[1.0, 0.0], &[]), 0.0);
    }

    #[test]
    fn different_lengths_score_zero() {
        assert_eq!(person_score(&[1.0, 0.0], &[vec![1.0, 0.0, 0.0]]), 0.0);
    }

    #[test]
    fn assignment_is_one_to_one_best_first() {
        // "a" is closest to both people; it takes p1 (1.0), so p3 goes to "b" (0.98) even though
        // "a" scores 0.99 with p3 too. "b" is listed first to show input order does not matter.
        let speakers = vec![speaker("b", &[0.95, 0.312]), speaker("a", &[1.0, 0.0])];
        let people = vec![person("p1", &[&[1.0, 0.0]]), person("p3", &[&[0.99, 0.141]])];

        let matches = assign_voices(&speakers, &people, &none_rejected(), VOICE_STRONG, VOICE_WEAK);

        assert_eq!(pairs(&matches), vec![("a", "p1", MatchStrength::Strong), ("b", "p3", MatchStrength::Strong)]);
        assert!(matches[0].score >= matches[1].score);
        assert_eq!(matches[0].name, "P1");
    }

    #[test]
    fn strong_links_weak_suggests_below_nothing() {
        let speakers = vec![
            speaker("s1", &[1.0, 0.0, 0.0]),
            speaker("s2", &[0.0, 1.0, 0.0]),
            speaker("s3", &[-1.0, 0.0, 0.0]),
        ];
        let people = vec![person("p1", &[&[1.0, 0.0, 0.0]]), person("p2", &[&[0.0, 0.7, 0.714]])];

        let matches = assign_voices(&speakers, &people, &none_rejected(), 0.75, 0.60);

        assert_eq!(pairs(&matches), vec![("s1", "p1", MatchStrength::Strong), ("s2", "p2", MatchStrength::Weak)]);
        assert!((matches[1].score - 0.70).abs() < 0.01, "{}", matches[1].score);
    }

    #[test]
    fn rejected_pairs_are_skipped() {
        let speakers = vec![speaker("s1", &[1.0, 0.0])];
        let people = vec![person("p1", &[&[1.0, 0.0]]), person("p2", &[&[0.8, 0.6]])];
        let rejected = HashSet::from([("s1".to_string(), "p1".to_string())]);

        let matches = assign_voices(&speakers, &people, &rejected, 0.75, 0.60);

        assert_eq!(pairs(&matches), vec![("s1", "p2", MatchStrength::Strong)]);
    }

    #[test]
    fn people_already_linked_in_the_meeting_are_skipped() {
        let linked = VoiceSpeaker { display_name: Some("P1".into()), person_id: Some("p1".into()), ..speaker("s0", &[0.0, 1.0]) };
        let speakers = vec![linked, speaker("s1", &[1.0, 0.0])];
        let people = vec![person("p1", &[&[1.0, 0.0]])];

        assert!(assign_voices(&speakers, &people, &none_rejected(), VOICE_STRONG, VOICE_WEAK).is_empty());
    }

    #[test]
    fn named_and_handmade_speakers_are_skipped() {
        // A name from before people existed (no person), a hand-made speaker (no voice) and an
        // empty centroid never match, however close the voice.
        let legacy = VoiceSpeaker { display_name: Some("Ana".into()), ..speaker("s0", &[1.0, 0.0]) };
        let handmade = VoiceSpeaker { embedding: None, ..speaker("s1", &[]) };
        let empty = speaker("s2", &[]);
        let people = vec![person("p1", &[&[1.0, 0.0]])];

        assert!(assign_voices(&[legacy, handmade, empty], &people, &none_rejected(), VOICE_STRONG, VOICE_WEAK).is_empty());
    }

    #[test]
    fn voice_reason_has_two_decimals() {
        assert_eq!(voice_reason(0.6789), "voice match 0.68");
        assert_eq!(voice_reason(0.7), "voice match 0.70");
    }

    use crate::database::repositories::speaker::SpeakerLink;
    use crate::database::test_support::{
        migrated_pool, seed_meeting, seed_person, seed_speakers, stored_speaker,
    };
    use sqlx::SqlitePool;

    const NOAH: &str = "person-noah";

    fn unnamed(key: &str, embedding: &[f32]) -> NewSpeaker {
        NewSpeaker { key: key.into(), embedding: embedding.to_vec(), speech_seconds: 1.0, ..Default::default() }
    }

    fn named(key: &str, embedding: &[f32], name: &str, person_id: &str, source: NameSource) -> NewSpeaker {
        NewSpeaker {
            display_name: Some(name.into()),
            link: SpeakerLink { person_id: Some(person_id.into()), name_source: Some(source), ..Default::default() },
            ..unnamed(key, embedding)
        }
    }

    fn idle(_: &str) -> bool {
        false
    }

    fn link(meeting_id: &str, key: &str) -> PropagatedLink {
        PropagatedLink { meeting_id: meeting_id.into(), speaker_key: key.into(), person_id: NOAH.into() }
    }

    /// Noah, named by the user in meeting "a" with voice [1, 0, 0].
    async fn noah_named_in_a(pool: &SqlitePool) {
        seed_person(pool, NOAH, "Noah").await;
        seed_speakers(pool, "a", vec![named("spk_0", &[1.0, 0.0, 0.0], "Noah", NOAH, NameSource::User)]).await;
    }

    #[tokio::test]
    async fn propagation_links_only_unnamed_speakers_with_strong_matches() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        let legacy = NewSpeaker { display_name: Some("Bob".into()), ..unnamed("spk_1", &[1.0, 0.0, 0.0]) };
        seed_speakers(&pool, "b", vec![unnamed("spk_0", &[0.99, 0.1, 0.0]), legacy]).await;
        seed_speakers(&pool, "c", vec![unnamed("spk_0", &[0.7, 0.71, 0.0])]).await; // weak: 0.70
        seed_meeting(&pool, "d", &[]).await;
        sqlx::query("INSERT INTO meeting_speakers (meeting_id, speaker_key, created_at) VALUES ('d', 'spk_0', '2026-10-05T10:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();

        let links = propagate_person(&pool, NOAH, &idle).await.unwrap();

        assert_eq!(links, vec![link("b", "spk_0")]);
        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!(b0.display_name.as_deref(), Some("Noah"));
        assert_eq!(b0.link.person_id.as_deref(), Some(NOAH));
        assert_eq!(b0.link.name_source, Some(NameSource::Voice));
        let b1 = stored_speaker(&pool, "b", "spk_1").await;
        assert_eq!(b1.display_name.as_deref(), Some("Bob"));
        assert_eq!(b1.link, SpeakerLink::default());
        let c0 = stored_speaker(&pool, "c", "spk_0").await;
        assert_eq!((c0.display_name, c0.link), (None, SpeakerLink::default()), "weak matches are not propagated or suggested");
        let d0 = stored_speaker(&pool, "d", "spk_0").await;
        assert_eq!((d0.display_name, d0.link), (None, SpeakerLink::default()));
    }

    #[tokio::test]
    async fn propagation_skips_busy_meetings_and_rejections() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        for id in ["b", "c", "e"] {
            seed_speakers(&pool, id, vec![unnamed("spk_0", &[1.0, 0.0, 0.0])]).await;
        }
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::add_rejection_conn(&mut conn, "c", "spk_0", NOAH).await.unwrap();
        drop(conn);
        let busy = |m: &str| m == "b";

        let links = propagate_person(&pool, NOAH, &busy).await.unwrap();

        assert_eq!(links, vec![link("e", "spk_0")]);
        assert_eq!(stored_speaker(&pool, "b", "spk_0").await.display_name, None);
        assert_eq!(stored_speaker(&pool, "c", "spk_0").await.display_name, None);
    }

    #[tokio::test]
    async fn propagation_skips_meetings_where_the_person_is_linked() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        seed_speakers(
            &pool,
            "b",
            vec![named("spk_0", &[0.0, 1.0, 0.0], "Noah", NOAH, NameSource::Voice), unnamed("spk_1", &[1.0, 0.0, 0.0])],
        )
        .await;

        let links = propagate_person(&pool, NOAH, &idle).await.unwrap();

        assert!(links.is_empty());
        assert_eq!(stored_speaker(&pool, "b", "spk_1").await.display_name, None);
    }

    #[tokio::test]
    async fn undo_unlinks_exactly_the_listed_links() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        seed_speakers(&pool, "b", vec![unnamed("spk_0", &[1.0, 0.0, 0.0])]).await;
        seed_speakers(&pool, "c", vec![unnamed("spk_0", &[0.99, 0.1, 0.0])]).await;
        let links = propagate_person(&pool, NOAH, &idle).await.unwrap();
        assert_eq!(links, vec![link("b", "spk_0"), link("c", "spk_0")]);

        let changed = undo_propagation(&pool, &links[..1]).await.unwrap();

        assert_eq!(changed, vec!["b".to_string()]);
        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!((b0.display_name, b0.link), (None, SpeakerLink::default()));
        assert_eq!(stored_speaker(&pool, "c", "spk_0").await.display_name.as_deref(), Some("Noah"));
        assert_eq!(stored_speaker(&pool, "a", "spk_0").await.display_name.as_deref(), Some("Noah"));
    }

    #[tokio::test]
    async fn undo_leaves_links_changed_since() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        seed_speakers(&pool, "b", vec![unnamed("spk_0", &[1.0, 0.0, 0.0])]).await;
        let links = propagate_person(&pool, NOAH, &idle).await.unwrap();
        SpeakersRepository::confirm(&pool, "b", "spk_0", true).await.unwrap();

        let changed = undo_propagation(&pool, &links).await.unwrap();

        assert!(changed.is_empty());
        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!(b0.display_name.as_deref(), Some("Noah"));
        assert_eq!(b0.link.name_source, Some(NameSource::User));
    }

    #[tokio::test]
    async fn a_rejected_propagated_person_does_not_return_as_a_suggestion() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        seed_speakers(&pool, "b", vec![unnamed("spk_0", &[1.0, 0.0, 0.0])]).await;
        sqlx::query(
            "UPDATE meeting_speakers SET suggested_person_id = ?, suggested_name = 'Noah', suggestion_source = 'voice',
                 suggestion_reason = 'sounds like Noah'
             WHERE meeting_id = 'b' AND speaker_key = 'spk_0'",
        )
        .bind(NOAH)
        .execute(&pool)
        .await
        .unwrap();

        propagate_person(&pool, NOAH, &idle).await.unwrap();
        let linked = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!(linked.link.suggested_name, None, "the suggestion naming the linked person is cleared");
        assert_eq!(linked.link.suggested_person_id, None);

        SpeakersRepository::reject(&pool, "b", "spk_0").await.unwrap();

        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!((b0.display_name, b0.link), (None, SpeakerLink::default()));
        let mut conn = pool.acquire().await.unwrap();
        let rejected = PeopleRepository::rejections_conn(&mut conn, "b").await.unwrap();
        assert!(rejected.contains(&("spk_0".to_string(), NOAH.to_string())), "{rejected:?}");
    }

    #[tokio::test]
    async fn propagation_and_undo_leave_suggestions_and_record_no_rejection() {
        let pool = migrated_pool().await;
        noah_named_in_a(&pool).await;
        seed_speakers(&pool, "b", vec![unnamed("spk_0", &[1.0, 0.0, 0.0])]).await;
        sqlx::query(
            "UPDATE meeting_speakers SET suggested_name = 'Ana', suggestion_source = 'conversation',
                 suggestion_reason = 'addressed as Ana at 00:10'
             WHERE meeting_id = 'b' AND speaker_key = 'spk_0'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let links = propagate_person(&pool, NOAH, &idle).await.unwrap();

        assert_eq!(links, vec![link("b", "spk_0")]);
        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!(b0.display_name.as_deref(), Some("Noah"));
        assert_eq!(b0.link.suggested_name.as_deref(), Some("Ana"), "propagation keeps the suggestion");
        assert_eq!(b0.link.suggestion_source, Some(SuggestionSource::Conversation));

        assert_eq!(undo_propagation(&pool, &links).await.unwrap(), vec!["b".to_string()]);

        let b0 = stored_speaker(&pool, "b", "spk_0").await;
        assert_eq!((b0.display_name, b0.link.person_id, b0.link.name_source), (None, None, None));
        assert_eq!(b0.link.suggested_name.as_deref(), Some("Ana"), "undo keeps the suggestion");
        assert_eq!(b0.link.suggestion_reason.as_deref(), Some("addressed as Ana at 00:10"));
        let mut conn = pool.acquire().await.unwrap();
        assert!(PeopleRepository::rejections_conn(&mut conn, "b").await.unwrap().is_empty(), "undo records no rejection");
    }
}

/// Measures how voices of the same person and of different people score, to choose the voice
/// thresholds. Reads the isolated dev database only, read-only.
#[cfg(test)]
mod calibration {
    use super::person_score;
    use crate::database::repositories::speaker::blob_to_embedding;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::collections::{BTreeMap, HashSet};
    use std::path::PathBuf;

    /// Highest strong threshold proposed: only the same recording scores 1.0.
    const MAX_STRONG: f32 = 0.99;
    /// Highest different-person pairs listed, so a voice split into two speakers can be spotted.
    const TOP_PAIRS: usize = 5;

    struct Stats {
        count: usize,
        min: f32,
        p05: f32,
        median: f32,
        p95: f32,
        max: f32,
    }

    fn stats(scores: &[f32]) -> Option<Stats> {
        if scores.is_empty() {
            return None;
        }
        let mut s = scores.to_vec();
        s.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| s[((s.len() - 1) as f64 * q).round() as usize];
        Some(Stats { count: s.len(), min: s[0], p05: at(0.05), median: at(0.5), p95: at(0.95), max: s[s.len() - 1] })
    }

    /// Rounds up to two decimals, ignoring float noise just above a step (in f32, 0.62 + 0.05 is
    /// 0.6700000167, which must still give 0.67).
    fn ceil_hundredth(x: f32) -> f32 {
        ((x as f64 * 100.0 - 1e-4).ceil() / 100.0) as f32
    }

    /// (strong, weak): strong clears the given different-person score by 0.05 (at most
    /// MAX_STRONG); weak is that score, kept below strong.
    fn propose_thresholds(max_different: f32) -> (f32, f32) {
        let strong = ceil_hundredth(max_different + 0.05).min(MAX_STRONG);
        let weak = ceil_hundredth(max_different).min(strong - 0.01);
        (strong, weak)
    }

    /// Non-empty 0.05-wide buckets as (bucket start, count).
    fn histogram(scores: &[f32]) -> Vec<(f32, usize)> {
        let mut buckets: BTreeMap<i32, usize> = BTreeMap::new();
        for &s in scores {
            *buckets.entry((s / 0.05).floor() as i32).or_default() += 1;
        }
        buckets.into_iter().map(|(b, n)| (b as f32 * 0.05, n)).collect()
    }

    fn print_class(label: &str, scores: &[f32]) {
        match stats(scores) {
            Some(s) => println!(
                "{label}: count {} | min {:.3} | p05 {:.3} | median {:.3} | p95 {:.3} | max {:.3}",
                s.count, s.min, s.p05, s.median, s.p95, s.max
            ),
            None => println!("{label}: no pairs"),
        }
        for (start, n) in histogram(scores) {
            println!("  {:+.2}..{:+.2} {:>5} {}", start, start + 0.05, n, "#".repeat(n.min(60)));
        }
    }

    fn dev_database() -> PathBuf {
        std::env::var_os("SPEAKERS_DEV_DB").map(PathBuf::from).unwrap_or_else(|| {
            dirs::data_dir().expect("no data directory").join("com.meetily.ai.speakers-dev/meeting_minutes.sqlite")
        })
    }

    /// "meeting_id/speaker_key" items separated by commas: speakers left out of the measurement
    /// (for a voice that diarization split into two speakers).
    fn parse_excluded(list: &str) -> HashSet<(String, String)> {
        list.split(',')
            .filter_map(|item| item.trim().split_once('/'))
            .map(|(meeting, key)| (meeting.trim().to_string(), key.trim().to_string()))
            .collect()
    }

    #[test]
    fn threshold_proposal_rounds_up_and_keeps_weak_below_strong() {
        assert_eq!(propose_thresholds(0.62), (0.67, 0.62));
        assert_eq!(propose_thresholds(0.613), (0.67, 0.62));
        assert_eq!(propose_thresholds(0.70), (0.75, 0.70));
        assert_eq!(propose_thresholds(0.97), (0.99, 0.97), "strong stays below 1");
        let s = stats(&[0.5, 0.1, 0.9, 0.3, 0.7]).unwrap();
        assert_eq!((s.count, s.min, s.median, s.max), (5, 0.1, 0.5, 0.9));
        assert!(stats(&[]).is_none());
        let h = histogram(&[0.61, 0.64, 0.66]);
        assert_eq!(h.iter().map(|&(_, n)| n).collect::<Vec<_>>(), vec![2, 1]);
        assert!((h[0].0 - 0.60).abs() < 1e-6 && (h[1].0 - 0.65).abs() < 1e-6);
    }

    #[test]
    fn excluded_speakers_are_read_from_the_list() {
        let excluded = parse_excluded(" m1/spk_3, m2/spk_0 ,broken,");
        assert_eq!(
            excluded,
            HashSet::from([("m1".to_string(), "spk_3".to_string()), ("m2".to_string(), "spk_0".to_string())])
        );
        assert!(parse_excluded("").is_empty());
    }

    #[ignore]
    #[tokio::test]
    async fn calibrate_voice_thresholds() {
        let path = dev_database();
        let shown = path.to_string_lossy().to_string();
        assert!(!shown.contains("/com.meetily.ai/"), "refusing to read the installed app's database: {shown}");
        let options = SqliteConnectOptions::new().filename(&path).read_only(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap_or_else(|e| panic!("cannot open {shown} read-only: {e}"));
        let rows: Vec<(String, String, Option<String>, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT meeting_id, speaker_key, person_id, name_source, embedding FROM meeting_speakers WHERE embedding IS NOT NULL",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let excluded = parse_excluded(&std::env::var("CALIBRATION_EXCLUDE").unwrap_or_default());

        struct Voice {
            meeting: String,
            key: String,
            person: Option<String>,
            exemplar: bool,
            embedding: Vec<f32>,
        }
        let voices: Vec<Voice> = rows
            .into_iter()
            .filter(|(meeting, key, ..)| !excluded.contains(&(meeting.clone(), key.clone())))
            .map(|(meeting, key, person, source, blob)| Voice {
                exemplar: person.is_some() && source.as_deref() == Some("user"),
                meeting,
                key,
                person,
                embedding: blob_to_embedding(&blob),
            })
            .collect();
        let exemplars: Vec<&Voice> = voices.iter().filter(|v| v.exemplar).collect();

        // Same person: each exemplar against that person's exemplars from other meetings.
        let mut same = Vec::new();
        for v in &exemplars {
            let others: Vec<Vec<f32>> = exemplars
                .iter()
                .filter(|o| o.person == v.person && o.meeting != v.meeting)
                .map(|o| o.embedding.clone())
                .collect();
            if !others.is_empty() {
                same.push(person_score(&v.embedding, &others));
            }
        }
        // Different people: each exemplar against the other speakers of its own meeting, as
        // (score, meeting, named speaker, other speaker), highest first. Two exemplars of
        // different people in one meeting are one pair: only the one with the lower key scores it.
        let mut different: Vec<(f32, &str, &str, &str)> = Vec::new();
        for v in &exemplars {
            for o in voices
                .iter()
                .filter(|o| o.meeting == v.meeting && o.person != v.person && !(o.exemplar && o.key < v.key))
            {
                let score = person_score(&o.embedding, std::slice::from_ref(&v.embedding));
                different.push((score, v.meeting.as_str(), v.key.as_str(), o.key.as_str()));
            }
        }
        different.sort_by(|a, b| b.0.total_cmp(&a.0));
        let different_scores: Vec<f32> = different.iter().map(|d| d.0).collect();

        let people: HashSet<&Option<String>> = exemplars.iter().map(|v| &v.person).collect();
        let meetings: HashSet<&String> = exemplars.iter().map(|v| &v.meeting).collect();
        println!("Dev database: {shown}");
        if !excluded.is_empty() {
            println!("Left out: {excluded:?}");
        }
        println!("{} exemplars of {} people in {} meetings; {} embedded speakers in all", exemplars.len(), people.len(), meetings.len(), voices.len());
        print_class("Same person", &same);
        print_class("Different people", &different_scores);
        println!("Highest different-person pairs (one voice split into two speakers scores like the same person):");
        for (score, meeting, named, other) in different.iter().take(TOP_PAIRS) {
            println!("  {score:.3}  meeting {meeting}  {named} (named) vs {other}");
        }

        assert!(!same.is_empty(), "no same-person pairs: name the same person in at least two meetings");
        assert!(!different.is_empty(), "no different-person pairs: name people in meetings with other speakers");
        let report = |label: &str, basis: f32| {
            let (strong, weak) = propose_thresholds(basis);
            let below = same.iter().filter(|&&s| s < strong).count();
            let weak_only = same.iter().filter(|&&s| s >= weak && s < strong).count();
            println!("Proposed from the {label} different-person score ({basis:.3}): VOICE_STRONG = {strong:.2}, VOICE_WEAK = {weak:.2}");
            println!("  same-person scores below strong: {below} of {} ({weak_only} of them would be suggestions)", same.len());
        };
        report("highest", different[0].0);
        if let Some(second) = different.get(1) {
            report("second-highest", second.0);
        }
        println!("To leave a split voice out: CALIBRATION_EXCLUDE=meeting_id/speaker_key[,…]");
    }
}
