//! Manual end-to-end probe of the "Распознавать знакомые голоса" loop.
//!
//! Drives the real production path on a real recording: local CAM++ embeddings →
//! `review_identity(confirm, allow_learning)` → `build_profile` → `resolve_clusters`
//! on a *second* meeting whose clusters are the same voices from unseen audio.
//!
//! Ignored by default (needs a recording and the diarization models). Run with:
//!
//! ```sh
//! VOICE_AUDIO=".../audio.mp4" VOICE_TURNS=".../transcripts.json" \
//! VOICE_MODELS="$HOME/Library/Application Support/com.meetily.ai/models/diarization" \
//! cargo test --test voice_learning_e2e -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use app_lib::learning::identity::{self, ReviewIdentityInput};
use app_lib::pipeline::diarization::{Diarizer, DiarizerConfig, SpeakerTurn};
use serde_json::Value;
use sqlx::Row;

/// Segments of one labeled speaker, in time order.
struct SpeakerSegments {
    label: String,
    spans: Vec<(i64, i64)>,
}

fn load_segments(path: &str) -> Vec<SpeakerSegments> {
    let raw = std::fs::read_to_string(path).expect("read transcripts.json");
    let parsed: Value = serde_json::from_str(&raw).expect("parse transcripts.json");
    let segments = parsed["segments"].as_array().expect("segments array");
    let mut by_speaker: BTreeMap<String, Vec<(i64, i64)>> = BTreeMap::new();
    for segment in segments {
        let (Some(speaker), Some(start), Some(end)) = (
            segment["speaker"].as_str(),
            segment["audio_start_time"].as_f64(),
            segment["audio_end_time"].as_f64(),
        ) else {
            continue;
        };
        let (start_ms, end_ms) = ((start * 1000.0) as i64, (end * 1000.0) as i64);
        if end_ms > start_ms {
            by_speaker
                .entry(speaker.to_string())
                .or_default()
                .push((start_ms, end_ms));
        }
    }
    by_speaker
        .into_iter()
        .map(|(label, mut spans)| {
            spans.sort_by_key(|span| span.0);
            SpeakerSegments { label, spans }
        })
        .collect()
}

/// Split one speaker's speech in half along the timeline: the first half enrolls the
/// profile, the second half is unseen audio the profile has to recognise.
fn split_halves(spans: &[(i64, i64)]) -> (Vec<(i64, i64)>, Vec<(i64, i64)>) {
    let total: i64 = spans.iter().map(|(start, end)| end - start).sum();
    let mut used = 0;
    let mut first = Vec::new();
    let mut second = Vec::new();
    for span in spans {
        if used < total / 2 {
            first.push(*span);
        } else {
            second.push(*span);
        }
        used += span.1 - span.0;
    }
    (first, second)
}

fn turns(spans: &[(i64, i64)], cluster_id: i64) -> Vec<SpeakerTurn> {
    spans
        .iter()
        .map(|(start_ms, end_ms)| SpeakerTurn {
            start_ms: *start_ms,
            end_ms: *end_ms,
            cluster_id,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real recording plus the local diarization models"]
async fn known_voices_are_recognised_in_a_later_meeting() {
    let audio = PathBuf::from(std::env::var("VOICE_AUDIO").expect("VOICE_AUDIO"));
    let turns_path = std::env::var("VOICE_TURNS").expect("VOICE_TURNS");
    let model_dir = PathBuf::from(std::env::var("VOICE_MODELS").expect("VOICE_MODELS"));

    let speakers = load_segments(&turns_path);
    println!("\n=== labeled speakers in {} ===", turns_path);
    let mut enroll_turns = Vec::new();
    let mut holdout_turns = Vec::new();
    let mut labels = Vec::new();
    let mut names = Vec::new();
    for (index, speaker) in speakers.iter().enumerate() {
        let (first, second) = split_halves(&speaker.spans);
        let secs = |spans: &[(i64, i64)]| -> f64 {
            spans.iter().map(|(a, b)| (b - a) as f64).sum::<f64>() / 1000.0
        };
        println!(
            "  [{index}] {:<14} {:>3} segments  enroll {:>6.0}s / holdout {:>6.0}s",
            speaker.label,
            speaker.spans.len(),
            secs(&first),
            secs(&second)
        );
        let cluster_id = index as i64;
        enroll_turns.extend(turns(&first, cluster_id));
        holdout_turns.extend(turns(&second, cluster_id));
        labels.push(speaker.label.clone());
        // Confirm under a human-looking name: "Speaker 182" matches the automatic-name
        // pattern the app reserves for unresolved placeholders.
        names.push(format!("Voice {}", (b'A' + index as u8) as char));
    }

    // Embedding decodes the whole recording twice; cache it so the DB half can be re-run.
    let cache = std::env::var("VOICE_CACHE").unwrap_or_default();
    let cached: Option<(Vec<(i64, Vec<f32>)>, Vec<(i64, Vec<f32>)>)> = (!cache.is_empty())
        .then(|| std::fs::read_to_string(&cache).ok())
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let (enroll_embeddings, holdout_embeddings) = match cached {
        Some(pair) => {
            println!("\nreusing cached embeddings from {cache}");
            pair
        }
        None => {
            let diarizer =
                Diarizer::load(DiarizerConfig { model_dir }).expect("load diarization models");
            println!("\nembedding enrollment halves…");
            let enroll = diarizer
                .embed_labeled_turns(&audio, &enroll_turns)
                .expect("embed enrollment turns");
            println!("embedding holdout halves…");
            let holdout = diarizer
                .embed_labeled_turns(&audio, &holdout_turns)
                .expect("embed holdout turns");
            if !cache.is_empty() {
                std::fs::write(&cache, serde_json::to_string(&(&enroll, &holdout)).unwrap())
                    .expect("write embedding cache");
            }
            (enroll, holdout)
        }
    };

    println!("\n=== raw same-voice / cross-voice cosine (production embedder) ===");
    let cos = |a: &[f32], b: &[f32]| -> f32 {
        let (dot, na, nb) = a.iter().zip(b).fold((0.0, 0.0, 0.0), |(d, x, y), (p, q)| {
            (d + p * q, x + p * p, y + q * q)
        });
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            dot / (f32::sqrt(na) * f32::sqrt(nb))
        }
    };
    print!("{:<14}", "enroll\\holdout");
    for (id, _) in &holdout_embeddings {
        print!("{:>10}", labels[*id as usize]);
    }
    println!();
    for (enroll_id, enroll_embedding) in &enroll_embeddings {
        print!("{:<14}", labels[*enroll_id as usize]);
        for (holdout_id, holdout_embedding) in &holdout_embeddings {
            let score = cos(enroll_embedding, holdout_embedding);
            let same = if enroll_id == holdout_id { "*" } else { " " };
            print!("{score:>9.3}{same}");
        }
        println!();
    }
    println!(
        "  enrolled {} voices, held out {} voices",
        enroll_embeddings.len(),
        holdout_embeddings.len()
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("voice.sqlite");
    let manager = app_lib::database::manager::DatabaseManager::new(
        db_path.to_str().unwrap(),
        db_path.to_str().unwrap(),
    )
    .await
    .expect("migrated database");
    let pool = manager.pool();

    for meeting in ["enroll-meeting", "holdout-meeting"] {
        sqlx::query(
            "INSERT INTO meetings(id, title, created_at, updated_at) \
             VALUES(?, ?, datetime('now'), datetime('now'))",
        )
        .bind(meeting)
        .bind(meeting)
        .execute(pool)
        .await
        .expect("insert meeting");
    }

    // 1) First meeting: clusters resolve to local Unknowns, then the user confirms each
    //    voice and ticks "remember this voice" — the only path that may train a profile.
    let enrolled = identity::resolve_clusters(
        pool,
        "enroll-meeting",
        "enroll-run",
        &enroll_turns,
        &enroll_embeddings,
    )
    .await
    .expect("resolve enrollment clusters");
    for (local_cluster_id, (_, cluster_id)) in &enrolled {
        identity::review_identity(
            pool,
            ReviewIdentityInput {
                cluster_id: *cluster_id,
                decision: "confirm".to_string(),
                speaker_id: None,
                display_name: Some(names[*local_cluster_id as usize].clone()),
                rejected_speaker_id: None,
                allow_learning: true,
                scope: "cluster".to_string(),
            },
        )
        .await
        .expect("confirm identity with learning");
    }
    let samples = sqlx::query(
        "SELECT s.display_name, vs.duration_ms, vs.speech_quality, vs.overlap_ratio, \
                vs.eligibility, COALESCE(vs.exclusion_reason,'-') AS exclusion_reason \
         FROM voice_samples vs JOIN speakers s ON s.id=vs.speaker_id ORDER BY vs.id",
    )
    .fetch_all(pool)
    .await
    .expect("read voice samples");
    println!("\n=== voice samples stored by the confirmations ===");
    for row in &samples {
        println!(
            "  {:<14} {:>6.0}s quality={:.2} overlap={:.2} {} ({})",
            row.get::<String, _>("display_name"),
            row.get::<i64, _>("duration_ms") as f64 / 1000.0,
            row.get::<f64, _>("speech_quality"),
            row.get::<f64, _>("overlap_ratio"),
            row.get::<String, _>("eligibility"),
            row.get::<String, _>("exclusion_reason"),
        );
    }

    let speaker_rows = sqlx::query(
        "SELECT s.id, s.display_name, s.is_confirmed, s.learning_enabled, s.consent_state, \
                s.profile_version, s.deleted_at IS NOT NULL AS deleted, \
                (SELECT COUNT(*) FROM voice_centroids vc WHERE vc.speaker_id=s.id) AS centroids, \
                (SELECT COUNT(*) FROM voice_centroids vc WHERE vc.speaker_id=s.id AND vc.is_active=1) AS active, \
                (SELECT COUNT(*) FROM speaker_profile_versions pv WHERE pv.speaker_id=s.id) AS versions \
         FROM speakers s ORDER BY s.id",
    )
    .fetch_all(pool)
    .await
    .expect("read speakers");
    println!("\n=== speaker rows after the confirmations ===");
    println!(
        "{:>3} {:<14} {:>9} {:>8} {:<9} {:>7} {:>9} {:>6} {:>8}",
        "id",
        "name",
        "confirmed",
        "learning",
        "consent",
        "version",
        "centroids",
        "active",
        "versions"
    );
    for row in &speaker_rows {
        println!(
            "{:>3} {:<14} {:>9} {:>8} {:<9} {:>7} {:>9} {:>6} {:>8}",
            row.get::<i64, _>("id"),
            row.get::<String, _>("display_name"),
            row.get::<i64, _>("is_confirmed"),
            row.get::<i64, _>("learning_enabled"),
            row.get::<String, _>("consent_state"),
            row.get::<i64, _>("profile_version"),
            row.get::<i64, _>("centroids"),
            row.get::<i64, _>("active"),
            row.get::<i64, _>("versions"),
        );
    }

    // review_identity swallows a failed rebuild ("confirmed without a profile rebuild").
    // Re-run it here for every speaker that ended up without centroids to see the error.
    println!("\n=== retrying build_profile for speakers with no centroids ===");
    for row in &speaker_rows {
        if row.get::<i64, _>("active") > 0 {
            continue;
        }
        let id: i64 = row.get("id");
        match identity::build_profile(pool, id, "probe_retry").await {
            Ok(version) => println!(
                "  speaker {id:>3} {:<14} rebuilt as version {version}",
                row.get::<String, _>("display_name")
            ),
            Err(error) => println!(
                "  speaker {id:>3} {:<14} FAILED: {error}",
                row.get::<String, _>("display_name")
            ),
        }
    }

    let profiles: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT s.id, s.display_name, COUNT(vc.id) FROM speakers s \
         JOIN voice_centroids vc ON vc.speaker_id=s.id AND vc.is_active=1 \
         WHERE s.learning_enabled=1 AND s.consent_state='granted' GROUP BY s.id",
    )
    .fetch_all(pool)
    .await
    .expect("read profiles");
    println!("\n=== learned profiles ===");
    for (id, name, centroids) in &profiles {
        println!("  speaker {id:>3} {name:<14} active centroids: {centroids}");
    }
    assert!(!profiles.is_empty(), "no profile was learned");

    // 2) Second meeting with the setting ON: the same voices, unseen audio.
    sqlx::query(
        "INSERT INTO app_settings_kv(key, value) VALUES('identity.auto_assign_enabled','true') \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .execute(pool)
    .await
    .expect("enable auto assignment");

    identity::resolve_clusters(
        pool,
        "holdout-meeting",
        "holdout-run",
        &holdout_turns,
        &holdout_embeddings,
    )
    .await
    .expect("resolve holdout clusters");

    let rows = sqlx::query(
        "SELECT sc.local_cluster_id, sc.speech_duration_ms, ir.policy_result, ir.top_score, \
                ir.top_margin, ir.candidate_scores_json, s.display_name AS operational \
         FROM speaker_clusters sc \
         JOIN identity_inference_runs ir ON ir.cluster_id=sc.id \
         LEFT JOIN speakers s ON s.id=sc.operational_speaker_id \
         WHERE sc.meeting_id='holdout-meeting' ORDER BY sc.local_cluster_id",
    )
    .fetch_all(pool)
    .await
    .expect("read holdout decisions");

    println!("\n=== recognition on unseen audio (auto-assignment ON) ===");
    println!(
        "{:<14} {:>6} {:<12} {:>6} {:>7}  {:<14} {}",
        "truth", "secs", "decision", "score", "margin", "top candidate", "correct"
    );
    let (mut auto, mut confirm, mut unknown, mut wrong) = (0, 0, 0, 0);
    for row in &rows {
        let local: i64 = row.get("local_cluster_id");
        let truth = &names[local as usize];
        let duration_ms: i64 = row.get("speech_duration_ms");
        let decision: String = row.get("policy_result");
        let top_score: Option<f64> = row.get("top_score");
        let margin: Option<f64> = row.get("top_margin");
        let candidates: Value =
            serde_json::from_str(&row.get::<String, _>("candidate_scores_json"))
                .unwrap_or(Value::Null);
        let top_name = candidates[0]["display_name"]
            .as_str()
            .unwrap_or("-")
            .to_string();
        let correct = match decision.as_str() {
            "unknown" => "-",
            _ if &top_name == truth => "yes",
            _ => "NO  <-- misidentified",
        };
        match decision.as_str() {
            "auto_assign" => auto += 1,
            "confirm" => confirm += 1,
            _ => unknown += 1,
        }
        if decision != "unknown" && &top_name != truth {
            wrong += 1;
        }
        println!(
            "{:<14} {:>6.0} {:<12} {:>6.3} {:>7.3}  {:<14} {}",
            truth,
            duration_ms as f64 / 1000.0,
            decision,
            top_score.unwrap_or(0.0),
            margin.unwrap_or(0.0),
            top_name,
            correct
        );
    }
    println!(
        "\nauto_assign={auto} confirm={confirm} unknown={unknown} misidentified={wrong} \
         (of {} voices)",
        rows.len()
    );
}
