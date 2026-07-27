// database/repositories/speaker_profile.rs
//
// CRUD for persistent voice profiles (speaker identification).
// Embeddings are stored as f32 little-endian BLOBs.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Error as SqlxError, FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct SpeakerProfileRow {
    pub id: String,
    pub name: String,
    pub embedding: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub embedding: Vec<f32>,
}

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Running-mean accrual of a saved profile centroid toward a newly-confirmed
/// cluster centroid, then re-normalized. `prior_count` is how many segments the
/// existing centroid already represents (weight of the old value).
pub fn accrue_centroid(existing: &[f32], prior_count: usize, new: &[f32]) -> Vec<f32> {
    if existing.len() != new.len() || existing.is_empty() {
        return existing.to_vec();
    }
    let w = prior_count.max(1) as f32;
    let mut out: Vec<f32> = existing
        .iter()
        .zip(new.iter())
        .map(|(e, n)| (e * w + n) / (w + 1.0))
        .collect();
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut out {
            *x /= norm;
        }
    }
    out
}

/// Default cap on stored exemplars per profile. Enough to cover within-speaker
/// variation (different mics, energy, rooms) without bloating match cost.
pub const DEFAULT_MAX_EXEMPLARS: usize = 6;

/// Cosine at/above which two exemplars belonging to *different* profiles are
/// treated as the same recording rather than two similar voices. Real distinct
/// enrollments never reach this: measured same-speaker exemplar pairs top out
/// well below it, whereas the known corruption cases sit at exactly 1.0.
pub const DUPLICATE_EXEMPLAR_THRESHOLD: f32 = 0.99;

/// Where an exemplar came from, so it can be withdrawn later.
///
/// Recorded when a rename enrolls a voice, and used to undo exactly that
/// contribution if the speaker is subsequently relabelled to someone else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExemplarSource {
    pub meeting_id: String,
    pub label: String,
}

/// A saved profile together with all of its stored raw voice exemplars.
#[derive(Debug, Clone)]
pub struct ProfileExemplars {
    pub id: String,
    pub name: String,
    pub exemplars: Vec<Vec<f32>>,
}

/// An exemplar shared by two different profiles — data corruption rather than
/// two people who merely sound alike. Surfaced for review before deletion.
#[derive(Debug, Clone)]
pub struct DuplicateExemplar {
    pub exemplar_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub other_profile_name: String,
    pub score: f32,
    /// True when the two blobs are bit-for-bit equal.
    pub identical: bool,
}

/// Cosine similarity for L2-normalized embeddings. Mismatched or empty inputs
/// score 0 rather than panicking.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// L2-normalized element-wise mean of equal-length embeddings — the "summary"
/// centroid kept on `speaker_profiles.embedding` for display and the raw
/// fallback path. Length-mismatched vectors are skipped; `None` if the input is
/// empty or the mean has zero norm.
pub fn mean_normalized(embeddings: &[Vec<f32>]) -> Option<Vec<f32>> {
    let dim = embeddings.iter().find(|e| !e.is_empty())?.len();
    let mut sum = vec![0.0f32; dim];
    let mut count = 0usize;
    for e in embeddings {
        if e.len() == dim {
            for (a, v) in sum.iter_mut().zip(e) {
                *a += v;
            }
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    for v in &mut sum {
        *v /= count as f32;
    }
    let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= 0.0 {
        return None;
    }
    for v in &mut sum {
        *v /= norm;
    }
    Some(sum)
}

/// When a profile exceeds its exemplar cap, pick which one to evict: the most
/// *redundant* exemplar — the one whose nearest neighbour among the others is
/// closest — so we shed a near-duplicate and keep diverse coverage rather than
/// blindly dropping the oldest. Assumes L2-normalized inputs. `None` if < 2.
pub fn most_redundant_exemplar_index(embeddings: &[Vec<f32>]) -> Option<usize> {
    if embeddings.len() < 2 {
        return None;
    }
    let cos = |a: &[f32], b: &[f32]| -> f32 {
        if a.len() != b.len() {
            return -1.0;
        }
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    };
    let mut worst: (usize, f32) = (0, f32::MIN);
    for (i, a) in embeddings.iter().enumerate() {
        let nearest = embeddings
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| cos(a, b))
            .fold(f32::MIN, f32::max);
        if nearest > worst.1 {
            worst = (i, nearest);
        }
    }
    Some(worst.0)
}

pub struct SpeakerProfilesRepository;

impl SpeakerProfilesRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<SpeakerProfile>, SqlxError> {
        let rows = sqlx::query_as::<_, SpeakerProfileRow>(
            "SELECT id, name, embedding FROM speaker_profiles ORDER BY name",
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SpeakerProfile {
                id: r.id,
                name: r.name,
                embedding: blob_to_embedding(&r.embedding),
            })
            .collect())
    }

    pub async fn create(
        pool: &SqlitePool,
        name: &str,
        embedding: &[f32],
        source: Option<&ExemplarSource>,
    ) -> Result<String, SqlxError> {
        let id = format!("speaker-{}", Uuid::new_v4());
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO speaker_profiles (id, name, embedding, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(embedding_to_blob(embedding))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        // Seed the profile's first exemplar (the summary starts equal to it).
        Self::insert_exemplar_row(pool, &id, embedding, source).await?;
        Ok(id)
    }

    pub async fn update_embedding(
        pool: &SqlitePool,
        id: &str,
        embedding: &[f32],
    ) -> Result<(), SqlxError> {
        sqlx::query("UPDATE speaker_profiles SET embedding = ?, updated_at = ? WHERE id = ?")
            .bind(embedding_to_blob(embedding))
            .bind(Utc::now())
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn rename(pool: &SqlitePool, id: &str, name: &str) -> Result<(), SqlxError> {
        sqlx::query("UPDATE speaker_profiles SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(Utc::now())
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), SqlxError> {
        // Delete exemplars explicitly (don't rely on FK cascade, which needs
        // PRAGMA foreign_keys=ON), then the profile row.
        sqlx::query("DELETE FROM speaker_profile_embeddings WHERE profile_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM speaker_profiles WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    // --- Multi-exemplar enrollment -----------------------------------------

    /// All saved profiles with their stored exemplars (for matching / flagging).
    /// Profiles with no exemplars are omitted (they can't be matched).
    pub async fn list_with_exemplars(
        pool: &SqlitePool,
    ) -> Result<Vec<ProfileExemplars>, SqlxError> {
        let rows = sqlx::query_as::<_, (String, String, Vec<u8>)>(
            "SELECT p.id, p.name, e.embedding \
             FROM speaker_profiles p \
             JOIN speaker_profile_embeddings e ON e.profile_id = p.id \
             ORDER BY p.id, e.created_at",
        )
        .fetch_all(pool)
        .await?;

        let mut out: Vec<ProfileExemplars> = Vec::new();
        for (id, name, blob) in rows {
            let emb = blob_to_embedding(&blob);
            match out.last_mut() {
                Some(p) if p.id == id => p.exemplars.push(emb),
                _ => out.push(ProfileExemplars {
                    id,
                    name,
                    exemplars: vec![emb],
                }),
            }
        }
        Ok(out)
    }

    /// (exemplar id, embedding) rows for one profile, oldest first.
    async fn exemplar_rows_for(
        pool: &SqlitePool,
        profile_id: &str,
    ) -> Result<Vec<(String, Vec<f32>)>, SqlxError> {
        let rows = sqlx::query_as::<_, (String, Vec<u8>)>(
            "SELECT id, embedding FROM speaker_profile_embeddings \
             WHERE profile_id = ? ORDER BY created_at",
        )
        .bind(profile_id)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, b)| (id, blob_to_embedding(&b)))
            .collect())
    }

    /// Add a new voice exemplar to a profile. If this pushes the profile over
    /// `max_exemplars`, the most-redundant exemplar is evicted. The profile's
    /// summary centroid (`speaker_profiles.embedding`) is then recomputed.
    pub async fn add_exemplar(
        pool: &SqlitePool,
        profile_id: &str,
        embedding: &[f32],
        max_exemplars: usize,
        source: Option<&ExemplarSource>,
    ) -> Result<(), SqlxError> {
        Self::insert_exemplar_row(pool, profile_id, embedding, source).await?;

        let rows = Self::exemplar_rows_for(pool, profile_id).await?;
        if rows.len() > max_exemplars.max(1) {
            let embs: Vec<Vec<f32>> = rows.iter().map(|(_, e)| e.clone()).collect();
            if let Some(idx) = most_redundant_exemplar_index(&embs) {
                sqlx::query("DELETE FROM speaker_profile_embeddings WHERE id = ?")
                    .bind(&rows[idx].0)
                    .execute(pool)
                    .await?;
            }
        }
        Self::refresh_summary(pool, profile_id).await
    }

    async fn insert_exemplar_row(
        pool: &SqlitePool,
        profile_id: &str,
        embedding: &[f32],
        source: Option<&ExemplarSource>,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT INTO speaker_profile_embeddings \
                 (id, profile_id, embedding, created_at, source_meeting_id, source_label) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("spx-{}", Uuid::new_v4()))
        .bind(profile_id)
        .bind(embedding_to_blob(embedding))
        .bind(Utc::now())
        .bind(source.map(|s| s.meeting_id.as_str()))
        .bind(source.map(|s| s.label.as_str()))
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Withdraw every exemplar a given meeting/label previously contributed,
    /// whichever profile now holds it, and refresh the affected summaries.
    /// Returns the names of the profiles that lost an exemplar.
    ///
    /// This is the un-enroll half of a rename. When a speaker is relabelled
    /// from one name to another, the vector the earlier rename donated must be
    /// taken back, or the abandoned profile keeps another person's voice
    /// forever — the defect that left "Alice" holding Camilia's embedding.
    ///
    /// It deliberately searches ALL profiles rather than one: the row to remove
    /// belongs to the profile being corrected *away from*, which by definition
    /// is not the profile being enrolled.
    ///
    /// Provenance is recorded under the label as it stands *after* the rename,
    /// because `relabel_and_merge_centroids` rewrites speakers.json in place —
    /// so the next rename of the same speaker arrives with that name as its
    /// `old_label`, and the chain lines up.
    ///
    /// Exemplars stored before provenance existed have NULL columns and never
    /// match here, so pre-existing data is left alone rather than guessed at.
    pub async fn withdraw_source(
        pool: &SqlitePool,
        source: &ExemplarSource,
    ) -> Result<Vec<String>, SqlxError> {
        let affected = sqlx::query_as::<_, (String, String)>(
            "SELECT DISTINCT p.id, p.name \
             FROM speaker_profile_embeddings e \
             JOIN speaker_profiles p ON p.id = e.profile_id \
             WHERE e.source_meeting_id = ? AND e.source_label = ?",
        )
        .bind(&source.meeting_id)
        .bind(&source.label)
        .fetch_all(pool)
        .await?;

        if affected.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query(
            "DELETE FROM speaker_profile_embeddings \
             WHERE source_meeting_id = ? AND source_label = ?",
        )
        .bind(&source.meeting_id)
        .bind(&source.label)
        .execute(pool)
        .await?;

        let mut names = Vec::new();
        for (id, name) in affected {
            Self::refresh_summary(pool, &id).await?;
            names.push(name);
        }
        Ok(names)
    }

    /// The saved profile (other than `exclude_profile_id`) that already owns an
    /// exemplar effectively identical to `embedding`, if any.
    ///
    /// Used to refuse an enrollment that would store one recording under two
    /// names — the failure mode that produced the Alice/Camilia and Nick/Ralf
    /// collisions. Returns `(profile name, score)` for the closest offender.
    pub async fn conflicting_profile(
        pool: &SqlitePool,
        embedding: &[f32],
        exclude_profile_id: Option<&str>,
        threshold: f32,
    ) -> Result<Option<(String, f32)>, SqlxError> {
        let mut best: Option<(String, f32)> = None;
        for p in Self::list_with_exemplars(pool).await? {
            if exclude_profile_id.is_some_and(|id| id == p.id) {
                continue;
            }
            for ex in &p.exemplars {
                let s = cosine(embedding, ex);
                if s >= threshold && best.as_ref().is_none_or(|(_, b)| s > *b) {
                    best = Some((p.name.clone(), s));
                }
            }
        }
        Ok(best)
    }

    /// Every exemplar that is also held, effectively identically, by a profile
    /// under a different name.
    ///
    /// Reported for review rather than deleted: a human decides which of the
    /// two names the recording actually belongs to. Each colliding pair yields
    /// two entries (one per profile) so either side can be chosen.
    pub async fn duplicate_exemplars(
        pool: &SqlitePool,
        threshold: f32,
    ) -> Result<Vec<DuplicateExemplar>, SqlxError> {
        // Pull ids alongside embeddings so a specific row can be deleted later.
        let rows = sqlx::query_as::<_, (String, String, String, Vec<u8>)>(
            "SELECT e.id, p.id, p.name, e.embedding \
             FROM speaker_profile_embeddings e \
             JOIN speaker_profiles p ON p.id = e.profile_id \
             ORDER BY p.name, e.created_at",
        )
        .fetch_all(pool)
        .await?;

        let parsed: Vec<(String, String, String, Vec<f32>, Vec<u8>)> = rows
            .into_iter()
            .map(|(eid, pid, name, blob)| {
                let emb = blob_to_embedding(&blob);
                (eid, pid, name, emb, blob)
            })
            .collect();

        let mut out = Vec::new();
        for i in 0..parsed.len() {
            for j in (i + 1)..parsed.len() {
                if parsed[i].1 == parsed[j].1 {
                    continue; // same profile — duplicates there are harmless
                }
                let score = cosine(&parsed[i].3, &parsed[j].3);
                if score < threshold {
                    continue;
                }
                let identical = parsed[i].4 == parsed[j].4;
                out.push(DuplicateExemplar {
                    exemplar_id: parsed[i].0.clone(),
                    profile_id: parsed[i].1.clone(),
                    profile_name: parsed[i].2.clone(),
                    other_profile_name: parsed[j].2.clone(),
                    score,
                    identical,
                });
                out.push(DuplicateExemplar {
                    exemplar_id: parsed[j].0.clone(),
                    profile_id: parsed[j].1.clone(),
                    profile_name: parsed[j].2.clone(),
                    other_profile_name: parsed[i].2.clone(),
                    score,
                    identical,
                });
            }
        }
        Ok(out)
    }

    /// Delete one exemplar row by id and refresh its profile's summary.
    /// Only ever called after explicit user confirmation.
    pub async fn delete_exemplar(pool: &SqlitePool, exemplar_id: &str) -> Result<(), SqlxError> {
        let profile_id: Option<String> = sqlx::query_scalar(
            "SELECT profile_id FROM speaker_profile_embeddings WHERE id = ?",
        )
        .bind(exemplar_id)
        .fetch_optional(pool)
        .await?;

        sqlx::query("DELETE FROM speaker_profile_embeddings WHERE id = ?")
            .bind(exemplar_id)
            .execute(pool)
            .await?;

        if let Some(pid) = profile_id {
            Self::refresh_summary(pool, &pid).await?;
        }
        Ok(())
    }

    /// Recompute a profile's summary centroid as the normalized mean of its
    /// current exemplars (no-op if it somehow has none).
    async fn refresh_summary(pool: &SqlitePool, profile_id: &str) -> Result<(), SqlxError> {
        let exemplars: Vec<Vec<f32>> = Self::exemplar_rows_for(pool, profile_id)
            .await?
            .into_iter()
            .map(|(_, e)| e)
            .collect();
        if let Some(summary) = mean_normalized(&exemplars) {
            Self::update_embedding(pool, profile_id, &summary).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let embedding = vec![0.5f32, -1.25, 3.75, 0.0];
        let blob = embedding_to_blob(&embedding);
        assert_eq!(blob.len(), 16);
        assert_eq!(blob_to_embedding(&blob), embedding);
    }

    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    #[test]
    fn mean_normalized_is_unit_and_averages() {
        // Mean of (1,0) and (0,1) is (0.5,0.5) -> normalized (1/√2, 1/√2).
        let m = mean_normalized(&[unit(vec![1.0, 0.0]), unit(vec![0.0, 1.0])]).unwrap();
        let e = 1.0f32 / 2.0f32.sqrt();
        assert!((m[0] - e).abs() < 1e-6 && (m[1] - e).abs() < 1e-6);
        assert_eq!(mean_normalized(&[]), None);
    }

    #[test]
    fn most_redundant_picks_the_near_duplicate() {
        // Two near-identical vectors + one distinct: one of the duplicates is
        // the most redundant (highest nearest-neighbour cosine) and gets evicted.
        let a = unit(vec![1.0, 0.0, 0.0]);
        let a2 = unit(vec![0.98, 0.05, 0.0]);
        let b = unit(vec![0.0, 1.0, 0.0]);
        let idx = most_redundant_exemplar_index(&[a, a2, b]).unwrap();
        assert!(idx == 0 || idx == 1, "expected a duplicate (idx 0/1), got {idx}");
        assert_eq!(most_redundant_exemplar_index(&[unit(vec![1.0, 0.0])]), None);
    }
}

#[cfg(test)]
mod accrual_tests {
    use super::*;
    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }
    #[test]
    fn accrue_moves_toward_new_and_stays_unit() {
        let existing = unit(vec![1.0, 0.0, 0.0]);
        let new = unit(vec![0.0, 1.0, 0.0]);
        let out = accrue_centroid(&existing, 4, &new); // 4 prior segments
        let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "result must be unit-norm, got {norm}");
        // moved toward `new` on axis 1 but still dominated by `existing` on axis 0
        assert!(out[0] > out[1] && out[1] > 0.0);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    async fn test_pool() -> SqlitePool {
        // Single connection so the in-memory schema persists across queries.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn exemplar_lifecycle_create_add_cap_delete() {
        let pool = test_pool().await;

        // create() seeds the profile's first exemplar.
        let id =
            SpeakerProfilesRepository::create(&pool, "Alice", &unit(vec![1.0, 0.0, 0.0]), None)
                .await
                .unwrap();
        let listed = SpeakerProfilesRepository::list_with_exemplars(&pool)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Alice");
        assert_eq!(listed[0].exemplars.len(), 1);

        // Adding beyond a cap of 2 evicts, so the count stays at 2.
        SpeakerProfilesRepository::add_exemplar(&pool, &id, &unit(vec![0.0, 1.0, 0.0]), 2, None)
            .await
            .unwrap();
        SpeakerProfilesRepository::add_exemplar(&pool, &id, &unit(vec![0.0, 0.0, 1.0]), 2, None)
            .await
            .unwrap();
        let listed = SpeakerProfilesRepository::list_with_exemplars(&pool)
            .await
            .unwrap();
        assert_eq!(listed[0].exemplars.len(), 2, "should cap at max_exemplars");

        // The summary centroid is maintained (unit-norm) and delete cascades.
        let summary = SpeakerProfilesRepository::list(&pool).await.unwrap()[0]
            .embedding
            .clone();
        let norm = summary.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "summary must be unit-norm, got {norm}");

        SpeakerProfilesRepository::delete(&pool, &id).await.unwrap();
        assert!(SpeakerProfilesRepository::list_with_exemplars(&pool)
            .await
            .unwrap()
            .is_empty());
    }

    fn src(meeting: &str, label: &str) -> ExemplarSource {
        ExemplarSource {
            meeting_id: meeting.to_string(),
            label: label.to_string(),
        }
    }

    /// The Alice-holds-Camilia defect, end to end.
    ///
    /// A voice is saved under one name, the user realises it is someone else
    /// and relabels it. Before provenance existed, the first profile silently
    /// kept the vector forever and the two profiles then scored 1.0 against
    /// each other.
    #[tokio::test]
    async fn relabelling_a_speaker_withdraws_the_previous_enrollment() {
        let pool = test_pool().await;
        let voice = unit(vec![0.3, 0.9, 0.1]);

        // Mislabelled as Alice.
        SpeakerProfilesRepository::create(&pool, "Alice", &voice, Some(&src("m1", "Alice")))
            .await
            .unwrap();

        // Corrected to Camilia: withdraw first, then enroll under the new name.
        let withdrawn = SpeakerProfilesRepository::withdraw_source(&pool, &src("m1", "Alice"))
            .await
            .unwrap();
        assert_eq!(withdrawn, vec!["Alice".to_string()]);
        SpeakerProfilesRepository::create(&pool, "Camilia", &voice, Some(&src("m1", "Camilia")))
            .await
            .unwrap();

        let listed = SpeakerProfilesRepository::list_with_exemplars(&pool)
            .await
            .unwrap();
        let names: Vec<&str> = listed.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Camilia"],
            "Alice should retain no exemplars once corrected: {names:?}"
        );
    }

    /// Provenance is recorded under the post-rename name, so a second
    /// correction of the same speaker still finds the row to withdraw.
    #[tokio::test]
    async fn a_second_correction_still_withdraws() {
        let pool = test_pool().await;
        let voice = unit(vec![0.1, 0.2, 0.97]);

        SpeakerProfilesRepository::create(&pool, "Alice", &voice, Some(&src("m1", "Alice")))
            .await
            .unwrap();
        SpeakerProfilesRepository::withdraw_source(&pool, &src("m1", "Alice"))
            .await
            .unwrap();
        SpeakerProfilesRepository::create(&pool, "Camilia", &voice, Some(&src("m1", "Camilia")))
            .await
            .unwrap();

        // Wrong again — it was really Dean.
        let withdrawn = SpeakerProfilesRepository::withdraw_source(&pool, &src("m1", "Camilia"))
            .await
            .unwrap();
        assert_eq!(withdrawn, vec!["Camilia".to_string()]);
        SpeakerProfilesRepository::create(&pool, "Dean", &voice, Some(&src("m1", "Dean")))
            .await
            .unwrap();

        let listed = SpeakerProfilesRepository::list_with_exemplars(&pool)
            .await
            .unwrap();
        let names: Vec<&str> = listed.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Dean"], "only the final name should hold the voice");
    }

    /// Exemplars saved before provenance existed carry NULL columns. Those must
    /// never be matched by a withdrawal, or a rename would delete an unrelated
    /// profile's data.
    #[tokio::test]
    async fn withdrawal_ignores_exemplars_without_provenance() {
        let pool = test_pool().await;
        SpeakerProfilesRepository::create(&pool, "Legacy", &unit(vec![1.0, 0.0, 0.0]), None)
            .await
            .unwrap();

        let withdrawn = SpeakerProfilesRepository::withdraw_source(&pool, &src("m1", "Legacy"))
            .await
            .unwrap();
        assert!(withdrawn.is_empty(), "legacy rows must be left alone");
        assert_eq!(
            SpeakerProfilesRepository::list_with_exemplars(&pool)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn duplicate_exemplars_are_detected_across_profiles_only() {
        let pool = test_pool().await;
        let voice = unit(vec![0.5, 0.5, 0.7]);

        let alice = SpeakerProfilesRepository::create(&pool, "Alice", &voice, None)
            .await
            .unwrap();
        SpeakerProfilesRepository::create(&pool, "Camilia", &voice, None)
            .await
            .unwrap();
        // A near-duplicate WITHIN one profile is normal and must not be reported.
        SpeakerProfilesRepository::add_exemplar(&pool, &alice, &voice, 6, None)
            .await
            .unwrap();

        let dupes =
            SpeakerProfilesRepository::duplicate_exemplars(&pool, DUPLICATE_EXEMPLAR_THRESHOLD)
                .await
                .unwrap();

        assert!(!dupes.is_empty(), "the cross-profile duplicate should be found");
        assert!(
            dupes.iter().all(|d| d.profile_name != d.other_profile_name),
            "within-profile duplicates must not be reported: {dupes:?}"
        );
        assert!(
            dupes.iter().any(|d| d.identical),
            "identical blobs should be marked as such: {dupes:?}"
        );
        // Both sides are offered so the user can choose which to drop.
        let names: std::collections::BTreeSet<&str> =
            dupes.iter().map(|d| d.profile_name.as_str()).collect();
        assert!(names.contains("Alice") && names.contains("Camilia"));
    }

    #[tokio::test]
    async fn conflicting_profile_finds_the_other_owner_and_skips_self() {
        let pool = test_pool().await;
        let voice = unit(vec![0.2, 0.3, 0.93]);
        let alice = SpeakerProfilesRepository::create(&pool, "Alice", &voice, None)
            .await
            .unwrap();

        // Enrolling the same recording elsewhere is a conflict...
        let hit = SpeakerProfilesRepository::conflicting_profile(
            &pool,
            &voice,
            None,
            DUPLICATE_EXEMPLAR_THRESHOLD,
        )
        .await
        .unwrap();
        assert_eq!(hit.map(|(n, _)| n), Some("Alice".to_string()));

        // ...but adding another exemplar to Alice herself is not.
        let self_hit = SpeakerProfilesRepository::conflicting_profile(
            &pool,
            &voice,
            Some(&alice),
            DUPLICATE_EXEMPLAR_THRESHOLD,
        )
        .await
        .unwrap();
        assert!(self_hit.is_none(), "a profile cannot conflict with itself");

        // A genuinely different voice is not a conflict.
        let other = SpeakerProfilesRepository::conflicting_profile(
            &pool,
            &unit(vec![1.0, 0.0, 0.0]),
            None,
            DUPLICATE_EXEMPLAR_THRESHOLD,
        )
        .await
        .unwrap();
        assert!(other.is_none());
    }

    #[tokio::test]
    async fn deleting_an_exemplar_refreshes_the_summary() {
        let pool = test_pool().await;
        let a = unit(vec![1.0, 0.0, 0.0]);
        let b = unit(vec![0.0, 1.0, 0.0]);
        let id = SpeakerProfilesRepository::create(&pool, "Alice", &a, None)
            .await
            .unwrap();
        SpeakerProfilesRepository::add_exemplar(&pool, &id, &b, 6, None)
            .await
            .unwrap();

        // Find the exemplar holding `b` and remove it.
        let rows = SpeakerProfilesRepository::exemplar_rows_for(&pool, &id)
            .await
            .unwrap();
        let target = rows
            .iter()
            .find(|(_, e)| cosine(e, &b) > 0.99)
            .expect("b should be stored")
            .0
            .clone();
        SpeakerProfilesRepository::delete_exemplar(&pool, &target)
            .await
            .unwrap();

        let summary = SpeakerProfilesRepository::list(&pool).await.unwrap()[0]
            .embedding
            .clone();
        assert!(
            cosine(&summary, &a) > 0.99,
            "summary should fall back to the remaining exemplar"
        );
    }
}
