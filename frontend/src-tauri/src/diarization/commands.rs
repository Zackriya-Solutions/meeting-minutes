// diarization/commands.rs
//
// Tauri command surface for speaker identification: feature toggle
// (persisted in the diarization_settings table), model status, and
// model download.

use crate::database::repositories::speaker_profile::{
    ExemplarSource, SpeakerProfilesRepository, DEFAULT_MAX_EXEMPLARS,
    DUPLICATE_EXEMPLAR_THRESHOLD,
};
use crate::state::AppState;
use sqlx::SqlitePool;
use tauri::{command, AppHandle, Manager, Runtime};

pub async fn is_enabled(pool: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT enabled FROM diarization_settings WHERE id = '1'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|v| v != 0)
        .unwrap_or(false)
}

#[command]
pub async fn diarization_get_status<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let enabled = is_enabled(state.db_manager.pool()).await;
    let model_present = super::models::is_embedding_model_present(&app);
    Ok(serde_json::json!({
        "enabled": enabled,
        "model_present": model_present,
        "model_filename": super::models::EMBEDDING_MODEL_FILENAME,
    }))
}

#[command]
pub async fn diarization_set_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO diarization_settings (id, enabled) VALUES ('1', $1)
        ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled
        "#,
    )
    .bind(enabled as i64)
    .execute(state.db_manager.pool())
    .await
    .map_err(|e| format!("Failed to save diarization setting: {}", e))?;
    log::info!("Speaker identification {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

#[command]
pub async fn diarization_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    super::models::download_embedding_model(&app).await
}

/// Read the centroid for a speaker label from a meeting folder's speakers.json.
fn load_centroid_from_folder(folder: &str, label: &str) -> Option<Vec<f32>> {
    let path = std::path::Path::new(folder).join("speakers.json");
    let content = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("speakers")?.as_array()?.iter().find_map(|s| {
        if s.get("label")?.as_str()? != label {
            return None;
        }
        let centroid: Vec<f32> = s
            .get("centroid")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();
        if centroid.is_empty() {
            None
        } else {
            Some(centroid)
        }
    })
}

/// Relabel `old_label` → `new_name` across (label, centroid, segments) speaker
/// entries, merging any that now share `new_name` into one: a segment-weighted
/// mean centroid, re-normalized, with segments summed. The merged entry keeps
/// the first occurrence's position. Pure (no I/O) so it is unit-tested.
pub(crate) fn relabel_and_merge_centroids(
    speakers: Vec<(String, Vec<f32>, usize)>,
    old_label: &str,
    new_name: &str,
) -> Vec<(String, Vec<f32>, usize)> {
    let mut out: Vec<(String, Vec<f32>, usize)> = Vec::new();
    let mut merged_idx: Option<usize> = None;
    for (label, centroid, segments) in speakers {
        let label = if label == old_label { new_name.to_string() } else { label };
        if label == new_name {
            match merged_idx {
                None => {
                    merged_idx = Some(out.len());
                    out.push((label, centroid, segments));
                }
                Some(i) => {
                    let existing = &mut out[i];
                    let w0 = existing.2.max(1) as f32;
                    let w1 = segments.max(1) as f32;
                    if existing.1.len() == centroid.len() {
                        for (a, b) in existing.1.iter_mut().zip(centroid.iter()) {
                            *a = (*a * w0 + *b * w1) / (w0 + w1);
                        }
                        let norm = existing.1.iter().map(|v| v * v).sum::<f32>().sqrt();
                        if norm > 0.0 {
                            for v in existing.1.iter_mut() {
                                *v /= norm;
                            }
                        }
                    }
                    existing.2 += segments;
                }
            }
        } else {
            out.push((label, centroid, segments));
        }
    }
    out
}

/// Best-effort: keep speakers.json labels in lockstep with a rename by
/// relabeling `old_label` → `new_name` (merging duplicates via
/// `relabel_and_merge_centroids`). No-op if the file is missing/unparseable or
/// `old_label` is not present — never fails the rename. Preserves the
/// `suggested` near-match hint on every surviving entry that isn't the one
/// just renamed, so confirming one speaker's suggestion doesn't wipe the
/// pending "Speaker N · Name?" hints on the others in the same meeting.
fn relabel_speaker_in_folder(folder: &str, old_label: &str, new_name: &str) {
    let path = std::path::Path::new(folder).join("speakers.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(_) => return,
    };
    let arr = match json.get("speakers").and_then(|s| s.as_array()) {
        Some(a) => a,
        None => return,
    };
    let mut suggestions: std::collections::HashMap<String, (String, f32)> = std::collections::HashMap::new();
    let speakers: Vec<(String, Vec<f32>, usize)> = arr
        .iter()
        .filter_map(|s| {
            let label = s.get("label")?.as_str()?.to_string();
            let centroid: Vec<f32> = s
                .get("centroid")?
                .as_array()?
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            let segments = s.get("segments").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(sug) = s.get("suggested") {
                if let (Some(name), Some(score)) = (
                    sug.get("name").and_then(|n| n.as_str()),
                    sug.get("score").and_then(|v| v.as_f64()),
                ) {
                    suggestions.insert(label.clone(), (name.to_string(), score as f32));
                }
            }
            if centroid.is_empty() {
                None
            } else {
                Some((label, centroid, segments))
            }
        })
        .collect();
    if !speakers.iter().any(|(l, _, _)| l == old_label) {
        return; // nothing to relabel
    }
    let relabeled = relabel_and_merge_centroids(speakers, old_label, new_name);
    let out = serde_json::json!({
        "version": "1.0",
        "speakers": relabeled.into_iter().map(|(label, centroid, segments)| {
            let mut entry = serde_json::json!({ "label": label, "centroid": centroid, "segments": segments });
            if let Some((name, score)) = suggestions.get(&label) {
                entry["suggested"] = serde_json::json!({ "name": name, "score": score });
            }
            entry
        }).collect::<Vec<_>>(),
    });
    match serde_json::to_string(&out).map(|s| std::fs::write(&path, s)) {
        Ok(Ok(())) => log::info!(
            "🎙️ Relabeled speakers.json '{}' → '{}' in {}",
            old_label, new_name, folder
        ),
        Ok(Err(e)) => log::warn!("🎙️ Failed to write speakers.json after relabel: {}", e),
        Err(e) => log::warn!("🎙️ Failed to serialize speakers.json after relabel: {}", e),
    }
}

/// Rename a speaker across all segments of a meeting. Optionally saves the
/// speaker's voice centroid (from the meeting's speakers.json) as a persistent
/// profile so future recordings label this voice by name automatically.
#[command]
pub async fn diarization_rename_speaker(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    old_label: String,
    new_name: String,
    save_profile: bool,
) -> Result<serde_json::Value, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Speaker name cannot be empty".to_string());
    }
    let pool = state.db_manager.pool();

    let result = sqlx::query("UPDATE transcripts SET speaker = ? WHERE meeting_id = ? AND speaker = ?")
        .bind(new_name)
        .bind(&meeting_id)
        .bind(&old_label)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to rename speaker: {}", e))?;
    let updated = result.rows_affected();

    let folder_path: Option<String> = sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(&meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to look up meeting folder: {}", e))?
        .flatten();

    let mut profile_saved = false;
    let mut withdrawn: Vec<String> = Vec::new();
    let mut conflict: Option<(String, f32)> = None;
    if save_profile {
        if let Some(centroid) = folder_path
            .as_deref()
            .and_then(|f| load_centroid_from_folder(f, &old_label))
        {
            // Un-enroll first. If this speaker was already saved under a
            // different name from this same meeting, that earlier contribution
            // has to be withdrawn — otherwise correcting a mislabel leaves the
            // abandoned profile holding this person's voice permanently.
            withdrawn = SpeakerProfilesRepository::withdraw_source(
                pool,
                &ExemplarSource {
                    meeting_id: meeting_id.clone(),
                    label: old_label.clone(),
                },
            )
            .await
            .map_err(|e| format!("Failed to withdraw the previous voice exemplar: {}", e))?;
            for name in &withdrawn {
                if name != new_name {
                    log::info!(
                        "Withdrew the voice exemplar '{}' had received for '{}' in meeting {}",
                        name,
                        old_label,
                        meeting_id
                    );
                }
            }

            let profiles = SpeakerProfilesRepository::list(pool)
                .await
                .map_err(|e| format!("Failed to load voice profiles: {}", e))?;
            let existing = profiles.into_iter().find(|p| p.name == new_name);

            // Refuse to store one recording under two names. Distinct people
            // never reach this similarity; an exact match means the same
            // cluster is being enrolled twice.
            conflict = SpeakerProfilesRepository::conflicting_profile(
                pool,
                &centroid,
                existing.as_ref().map(|p| p.id.as_str()),
                DUPLICATE_EXEMPLAR_THRESHOLD,
            )
            .await
            .map_err(|e| format!("Failed to check for duplicate voice exemplars: {}", e))?;

            if let Some((other, score)) = &conflict {
                log::warn!(
                    "Not saving a voice profile for '{}': this recording is already saved as '{}' (score {:.4})",
                    new_name,
                    other,
                    score
                );
            } else {
                // Provenance is the label as it stands AFTER this rename,
                // because speakers.json is rewritten below — so a later rename
                // of the same speaker arrives with this name as its old_label.
                let source = ExemplarSource {
                    meeting_id: meeting_id.clone(),
                    label: new_name.to_string(),
                };

                if let Some(existing) = existing {
                    // Already have a profile for this name - append this cluster as a
                    // new voice exemplar (capped, most-redundant evicted) instead of
                    // blending it into one drifting centroid or inserting a dup row.
                    SpeakerProfilesRepository::add_exemplar(
                        pool,
                        &existing.id,
                        &centroid,
                        DEFAULT_MAX_EXEMPLARS,
                        Some(&source),
                    )
                    .await
                    .map_err(|e| format!("Failed to update voice profile: {}", e))?;
                    log::info!("Added a voice exemplar to '{}' from meeting {}", new_name, meeting_id);
                } else {
                    SpeakerProfilesRepository::create(pool, new_name, &centroid, Some(&source))
                        .await
                        .map_err(|e| format!("Failed to save voice profile: {}", e))?;
                    log::info!("Saved voice profile '{}' from meeting {}", new_name, meeting_id);
                }
                profile_saved = true;
            }
        } else {
            log::warn!(
                "No voice centroid found for '{}' in meeting {} - profile not saved",
                old_label,
                meeting_id
            );
        }
    }

    // Keep speakers.json labels in lockstep with the transcript so a later
    // re-attribution can still find this voice under its current name.
    if let Some(folder) = folder_path.as_deref() {
        relabel_speaker_in_folder(folder, &old_label, new_name);
    }

    Ok(serde_json::json!({
        "updated_segments": updated,
        "profile_saved": profile_saved,
        // Names that lost the exemplar this speaker had previously donated —
        // shown so a correction visibly undoes the earlier mistake.
        "withdrawn_from": withdrawn.iter().filter(|n| *n != new_name).collect::<Vec<_>>(),
        // Set when enrollment was refused because the recording is already
        // saved under another name.
        "duplicate_of": conflict.as_ref().map(|(n, _)| n.clone()),
        "duplicate_score": conflict.as_ref().map(|(_, s)| *s),
    }))
}

/// Exemplars held by two profiles under different names — one recording saved
/// twice, rather than two people who sound alike.
///
/// Reported for review only; nothing is deleted until the user picks a row and
/// calls `diarization_delete_exemplar`. Returns
/// `[{ exemplarId, profileId, profileName, otherProfileName, score, identical }]`.
#[command]
pub async fn diarization_list_duplicate_exemplars(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let dupes = SpeakerProfilesRepository::duplicate_exemplars(
        state.db_manager.pool(),
        DUPLICATE_EXEMPLAR_THRESHOLD,
    )
    .await
    .map_err(|e| format!("Failed to scan for duplicate voice exemplars: {}", e))?;

    Ok(dupes
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "exemplarId": d.exemplar_id,
                "profileId": d.profile_id,
                "profileName": d.profile_name,
                "otherProfileName": d.other_profile_name,
                "score": d.score,
                "identical": d.identical,
            })
        })
        .collect())
}

/// Remove one voice exemplar by id and refresh its profile's summary.
/// Intended for confirmed cleanup of a duplicate surfaced above.
#[command]
pub async fn diarization_delete_exemplar(
    state: tauri::State<'_, AppState>,
    exemplar_id: String,
) -> Result<(), String> {
    SpeakerProfilesRepository::delete_exemplar(state.db_manager.pool(), &exemplar_id)
        .await
        .map_err(|e| format!("Failed to delete voice exemplar: {}", e))?;
    log::info!("Deleted voice exemplar {}", exemplar_id);
    Ok(())
}

#[command]
pub async fn diarization_list_profiles(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let profiles = SpeakerProfilesRepository::list(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to list voice profiles: {}", e))?;
    Ok(profiles
        .into_iter()
        .map(|p| serde_json::json!({ "id": p.id, "name": p.name }))
        .collect())
}

/// Flag saved voices that are confusable with another saved voice (a sign of a
/// contaminated profile). Returns `[{ name, confusedWith, score }]` — usually
/// empty. The UI surfaces these so the user can review/delete the offenders.
#[command]
pub async fn diarization_flag_confusable_profiles(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let profiles = SpeakerProfilesRepository::list_with_exemplars(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to load voice profiles: {}", e))?;
    let input: Vec<(String, Vec<Vec<f32>>)> =
        profiles.into_iter().map(|p| (p.name, p.exemplars)).collect();
    Ok(super::flagging::flag_confusable_profiles(&input)
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "name": f.name,
                "confusedWith": f.confused_with,
                "score": f.score,
            })
        })
        .collect())
}

#[command]
pub async fn diarization_rename_profile(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Profile name cannot be empty".to_string());
    }
    SpeakerProfilesRepository::rename(state.db_manager.pool(), &id, name)
        .await
        .map_err(|e| format!("Failed to rename voice profile: {}", e))
}

#[command]
pub async fn diarization_delete_profile(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    SpeakerProfilesRepository::delete(state.db_manager.pool(), &id)
        .await
        .map_err(|e| format!("Failed to delete voice profile: {}", e))
}

/// Create a diarization session for a recording/batch job when the feature is
/// enabled AND the embedding model is present. Seeds saved voice profiles so
/// returning speakers are labeled by name. Any failure returns None so speaker
/// labels are simply absent — transcription is never affected.
pub async fn init_session<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<super::DiarizationSession> {
    let enabled = match app.try_state::<AppState>() {
        Some(state) => is_enabled(state.db_manager.pool()).await,
        None => false,
    };
    if !enabled {
        log::info!("🎙️ Speaker identification disabled");
        return None;
    }
    if !super::models::is_embedding_model_present(app) {
        log::warn!("🎙️ Speaker identification enabled but embedding model not downloaded - labels disabled");
        return None;
    }
    let model_path = match super::models::embedding_model_path(app) {
        Ok(path) => path,
        Err(e) => {
            log::warn!("🎙️ Could not resolve diarization model path: {}", e);
            return None;
        }
    };

    let profiles: Vec<(String, Vec<Vec<f32>>)> = match app.try_state::<AppState>() {
        Some(state) => {
            match SpeakerProfilesRepository::list_with_exemplars(state.db_manager.pool()).await {
                Ok(profiles) => profiles.into_iter().map(|p| (p.name, p.exemplars)).collect(),
                Err(e) => {
                    log::warn!("🎙️ Failed to load voice profiles, continuing without: {}", e);
                    Vec::new()
                }
            }
        }
        None => Vec::new(),
    };
    let profile_count = profiles.len();

    match super::DiarizationSession::with_profiles(&model_path, profiles) {
        Ok(session) => {
            log::info!(
                "🎙️ ✅ Speaker identification active ({} saved profile{})",
                profile_count,
                if profile_count == 1 { "" } else { "s" }
            );
            Some(session)
        }
        Err(e) => {
            log::warn!("🎙️ Failed to initialize speaker identification: {}", e);
            None
        }
    }
}

/// Persist a session's speaker centroids to speakers.json in the meeting folder
/// so a later rename can save the voice as a persistent profile.
pub async fn persist_speaker_centroids(
    session: &super::DiarizationSession,
    folder: Option<std::path::PathBuf>,
) {
    persist_labeled_centroids(folder, &session.centroid_snapshot(), &std::collections::HashMap::new()).await;
}

/// Persist an explicit set of (label, centroid, unit count) speaker centroids
/// to speakers.json in the meeting folder. Used by the turn-based (sidecar)
/// batch diarization path, which maps local speakers to final profile-backed
/// labels itself rather than through a `DiarizationSession`'s online
/// clusterer. Same JSON shape as `persist_speaker_centroids` so rename /
/// "remember voice" (`load_centroid_from_folder`) reads it identically.
/// `suggestions` carries any near-match (name, score) found for a label,
/// stored alongside it as an optional `suggested` field for the frontend to
/// surface as a one-tap confirmation.
pub async fn persist_labeled_centroids(
    folder: Option<std::path::PathBuf>,
    centroids: &[(String, Vec<f32>, usize)],
    suggestions: &std::collections::HashMap<String, (String, f32)>,
) {
    if centroids.is_empty() {
        return;
    }
    let folder = match folder {
        Some(folder) => folder,
        None => {
            log::warn!("🎙️ No meeting folder available - speaker centroids not persisted");
            return;
        }
    };
    let json = serde_json::json!({
        "version": "1.0",
        "speakers": centroids.iter().map(|(label, centroid, count)| {
            let mut entry = serde_json::json!({ "label": label, "centroid": centroid, "segments": count });
            if let Some((name, score)) = suggestions.get(label) {
                entry["suggested"] = serde_json::json!({ "name": name, "score": score });
            }
            entry
        }).collect::<Vec<_>>(),
    });
    let path = folder.join("speakers.json");
    match serde_json::to_string(&json).map(|s| std::fs::write(&path, s)) {
        Ok(Ok(())) => log::info!("🎙️ Saved {} speaker centroid(s) to {}", centroids.len(), path.display()),
        Ok(Err(e)) => log::warn!("🎙️ Failed to write speakers.json: {}", e),
        Err(e) => log::warn!("🎙️ Failed to serialize speaker centroids: {}", e),
    }
}

#[derive(serde::Serialize)]
pub struct SuggestionDto {
    pub name: String,
    pub score: f32,
}

/// Best-effort: read per-cluster near-match suggestions from a meeting's
/// speakers.json. Returns { label -> {name, score} } for entries that carry a
/// `suggested` field. Empty map on any missing/unparseable data.
#[command]
pub async fn diarization_get_suggestions(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<std::collections::HashMap<String, SuggestionDto>, String> {
    let pool = state.db_manager.pool();
    let mut out = std::collections::HashMap::new();
    let folder_path: Option<String> = match sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(&meeting_id)
        .fetch_optional(pool)
        .await
    {
        Ok(row) => row.flatten(),
        Err(_) => return Ok(out),
    };
    let folder = match folder_path {
        Some(f) => f,
        None => return Ok(out),
    };
    let path = std::path::Path::new(&folder).join("speakers.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(out),
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(_) => return Ok(out),
    };
    if let Some(arr) = json.get("speakers").and_then(|s| s.as_array()) {
        for s in arr {
            if let (Some(label), Some(sug)) = (s.get("label").and_then(|l| l.as_str()), s.get("suggested")) {
                if let (Some(name), Some(score)) = (
                    sug.get("name").and_then(|n| n.as_str()),
                    sug.get("score").and_then(|v| v.as_f64()),
                ) {
                    out.insert(label.to_string(), SuggestionDto { name: name.to_string(), score: score as f32 });
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    #[test]
    fn relabel_simple_no_collision() {
        let speakers = vec![
            ("Speaker 1".to_string(), unit(vec![1.0, 0.0, 0.0]), 3),
            ("Speaker 2".to_string(), unit(vec![0.0, 1.0, 0.0]), 5),
        ];
        let out = relabel_and_merge_centroids(speakers, "Speaker 2", "Bob");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "Speaker 1");
        assert_eq!(out[1].0, "Bob");
        assert_eq!(out[1].2, 5); // segments preserved
    }

    #[test]
    fn relabel_merges_on_name_collision() {
        // "Speaker 2" is renamed to "Bob", but a "Bob" already exists → merge into one.
        let speakers = vec![
            ("Bob".to_string(), unit(vec![1.0, 0.0, 0.0]), 4),
            ("Speaker 2".to_string(), unit(vec![1.0, 0.2, 0.0]), 2),
        ];
        let out = relabel_and_merge_centroids(speakers, "Speaker 2", "Bob");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "Bob");
        assert_eq!(out[0].2, 6); // 4 + 2 segments summed
        let norm = out[0].1.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "merged centroid must be unit-norm, got {norm}");
    }

    #[test]
    fn relabel_absent_old_label_is_unchanged() {
        let speakers = vec![("Speaker 1".to_string(), unit(vec![1.0, 0.0, 0.0]), 3)];
        let out = relabel_and_merge_centroids(speakers.clone(), "Nope", "Bob");
        assert_eq!(out, speakers);
    }

    #[test]
    fn relabel_speaker_in_folder_preserves_other_suggestions() {
        // Renaming "Speaker 2" must not wipe "Speaker 1"'s pending suggestion,
        // and the renamed entry ("Bob") should lose its own since it's now named.
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let speakers_json = serde_json::json!({
            "version": "1.0",
            "speakers": [
                {
                    "label": "Speaker 1",
                    "centroid": unit(vec![1.0, 0.0, 0.0]),
                    "segments": 3,
                    "suggested": { "name": "Alice", "score": 0.81 },
                },
                {
                    "label": "Speaker 2",
                    "centroid": unit(vec![0.0, 1.0, 0.0]),
                    "segments": 5,
                    "suggested": { "name": "Bob", "score": 0.77 },
                },
            ],
        });
        std::fs::write(dir.path().join("speakers.json"), speakers_json.to_string())
            .expect("failed to write speakers.json");

        relabel_speaker_in_folder(dir.path().to_str().unwrap(), "Speaker 2", "Bob");

        let content = std::fs::read_to_string(dir.path().join("speakers.json"))
            .expect("failed to read speakers.json back");
        let json: serde_json::Value = serde_json::from_str(&content).expect("invalid JSON written");
        let arr = json["speakers"].as_array().expect("speakers must be an array");

        let speaker_1 = arr
            .iter()
            .find(|s| s["label"] == "Speaker 1")
            .expect("Speaker 1 entry missing");
        assert_eq!(speaker_1["suggested"]["name"], "Alice", "Speaker 1 must keep its suggestion");

        let bob = arr.iter().find(|s| s["label"] == "Bob").expect("renamed Bob entry missing");
        assert!(bob.get("suggested").is_none(), "renamed entry must not keep its own suggestion");
    }
}
