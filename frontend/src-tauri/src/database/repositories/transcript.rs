use crate::api::{TranscriptSearchResult, TranscriptSegment};
use crate::diarization::overlap_detector::{AttributionSource, OverlapStatus};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use std::collections::BTreeMap;
use tracing::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting '{}': {}", meeting_title, e);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let overlap_speaker_ids = serialize_overlap_speaker_ids(segment);
            let result = sqlx::query(
                "INSERT INTO transcripts (
                    id,
                    meeting_id,
                    transcript,
                    timestamp,
                    audio_start_time,
                    audio_end_time,
                    duration,
                    speaker,
                    attribution_source,
                    overlap_region_id,
                    overlap_speaker_ids,
                    overlap_start_time,
                    overlap_end_time,
                    overlap_confidence
                 )
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.speaker)
            .bind(
                segment
                    .attribution_source
                    .as_ref()
                    .map(AttributionSource::as_str),
            )
            .bind(&segment.overlap_region_id)
            .bind(overlap_speaker_ids)
            .bind(segment.overlap_start_time)
            .bind(segment.overlap_end_time)
            .bind(segment.overlap_confidence)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        for region in collect_overlap_regions(transcripts) {
            let result = sqlx::query(
                "INSERT INTO overlap_regions (
                    meeting_id,
                    id,
                    start_ms,
                    end_ms,
                    speaker_ids,
                    confidence,
                    estimated_speaker_count,
                    status
                 )
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&meeting_id)
            .bind(&region.id)
            .bind(region.start_ms as i64)
            .bind(region.end_ms as i64)
            .bind(&region.speaker_ids_json)
            .bind(region.confidence)
            .bind(region.estimated_speaker_count as i64)
            .bind(&region.status)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save overlap region {} for meeting {}: {}",
                    region.id, meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

struct PersistedOverlapRegion {
    id: String,
    start_ms: u64,
    end_ms: u64,
    speaker_ids_json: String,
    confidence: Option<f32>,
    estimated_speaker_count: usize,
    status: String,
}

fn serialize_overlap_speaker_ids(segment: &TranscriptSegment) -> Option<String> {
    segment
        .overlap_speaker_ids
        .as_ref()
        .and_then(|speaker_ids| serde_json::to_string(speaker_ids).ok())
}

fn collect_overlap_regions(transcripts: &[TranscriptSegment]) -> Vec<PersistedOverlapRegion> {
    let mut regions = BTreeMap::new();

    for segment in transcripts {
        let Some(region_id) = &segment.overlap_region_id else {
            continue;
        };

        let speaker_ids = segment.overlap_speaker_ids.clone().unwrap_or_default();
        let speaker_ids_json = serde_json::to_string(&speaker_ids).unwrap_or_else(|_| "[]".into());
        let status = segment
            .overlap_status
            .as_ref()
            .map(OverlapStatus::as_str)
            .or_else(|| {
                segment
                    .attribution_source
                    .as_ref()
                    .map(infer_overlap_status_from_attribution)
            })
            .unwrap_or("MarkedAmbiguous")
            .to_string();

        regions
            .entry(region_id.clone())
            .or_insert(PersistedOverlapRegion {
                id: region_id.clone(),
                start_ms: segment
                    .overlap_start_time
                    .map(seconds_to_ms)
                    .unwrap_or_default(),
                end_ms: segment
                    .overlap_end_time
                    .map(seconds_to_ms)
                    .unwrap_or_default(),
                speaker_ids_json,
                confidence: segment.overlap_confidence,
                estimated_speaker_count: speaker_ids.len(),
                status,
            });
    }

    regions.into_values().collect()
}

fn infer_overlap_status_from_attribution(source: &AttributionSource) -> &'static str {
    match source {
        AttributionSource::OverlapDetectedAmbiguous => "MarkedAmbiguous",
        AttributionSource::Level5Resolved => "Resolved",
        AttributionSource::UserCorrected => "Resolved",
        AttributionSource::NormalDiarization => "Detected",
    }
}

fn seconds_to_ms(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1000.0).round() as u64
}
