//! Speaker identification jobs: one at a time, queued, cancellable.
use super::assign::{carry_over, label_rows, split_text_at_turns, RowLabel, RowSpan, CARRY_OVER_MIN_SIMILARITY};
use super::diarizer::{Diarization, DiarizeOptions, Diarizer};
use super::models::{self, DownloadProgress};
use super::timing::{read_metadata, recording_time_map, TimeMap};
use super::naming::{propose_names, saved_model_is_local, summary_model_from_settings, KnownPerson, NamingInput, NamingLine, NamingModel, NamingSpeaker};
use super::{Cancelled, Turn};
use crate::api::TranscriptSegment;
use crate::audio::common::{
    acquire_batch_engine_lock, batch_engine_busy, unload_engine_after_batch, write_transcripts_json, BatchEngine,
};
use crate::audio::decoder::decode_audio_file;
use crate::database::models::Transcript;
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::person::{name_key, PeopleRepository};
use crate::database::repositories::speaker::{NewSpeaker, SpeakerLink, SpeakerWrite, SpeakersRepository, SplitRow, SuggestionSource};
use crate::database::repositories::summary::SummaryProcessesRepository;
use crate::state::AppState;
use crate::whisper_engine::WhisperCompiledBackend;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const PROGRESS_EVENT: &str = "diarization-progress";
pub const COMPLETE_EVENT: &str = "diarization-complete";
pub const ERROR_EVENT: &str = "diarization-error";
/// Automatic jobs are queued after the recording is saved, when its audio is normally final;
/// this only covers a file that is still being written.
const AUTO_AUDIO_WAIT: Duration = Duration::from_secs(30);
/// How often cancellable waits re-check the cancel flag.
const LOCK_POLL: Duration = Duration::from_millis(250);

/// What a queued job does with the meeting. Serialized as its name ("identify" or "naming").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobKind {
    /// Find who speaks when and label the rows.
    Identify { folder_path: PathBuf, num_speakers: Option<usize> },
    /// Ask the summary model for names said in the conversation. An automatic job sends the
    /// transcript to a model that is not local only with `allow_cloud`.
    Naming { allow_cloud: bool },
}

impl JobKind {
    pub fn name(&self) -> &'static str {
        match self {
            JobKind::Identify { .. } => "identify",
            JobKind::Naming { .. } => "naming",
        }
    }

    /// Refusal of a second job for a meeting that has this one.
    fn queued_message(&self) -> &'static str {
        match self {
            JobKind::Identify { .. } => "Speaker identification is already queued or running for this meeting",
            JobKind::Naming { .. } => "Finding names is already queued or running for this meeting",
        }
    }

    /// Refusal of a speaker edit while this job is queued or running.
    fn busy_message(&self) -> &'static str {
        match self {
            JobKind::Identify { .. } => IDENTIFYING_MESSAGE,
            JobKind::Naming { .. } => NAMING_MESSAGE,
        }
    }

    fn cancelled_message(&self) -> &'static str {
        match self {
            JobKind::Identify { .. } => "Speaker identification cancelled",
            JobKind::Naming { .. } => "Finding names cancelled",
        }
    }
}

impl Serialize for JobKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

#[derive(Debug, Clone)]
pub struct IdentifyRequest {
    pub meeting_id: String,
    pub automatic: bool,
    pub kind: JobKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatus {
    pub meeting_id: String,
    /// Which job this is, so the queued banner and the busy message can name it.
    pub kind: JobKind,
    pub state: JobState,
    /// What the job is doing: None while queued, then "audio", "waiting", "download",
    /// "segmentation", "embeddings", "clustering", "splitting", "saving", "naming" or "done".
    pub stage: Option<&'static str>,
    pub percent: u32,
    pub message: String,
}

/// Result of asking to cancel a meeting's job.
#[derive(Debug)]
pub enum CancelOutcome {
    /// The job had not started; it was removed from the queue.
    Queued(IdentifyRequest),
    /// The job is running; it stops at its next cancel check.
    Running,
    NotFound,
}

#[derive(Default)]
pub struct JobQueue {
    queue: VecDeque<IdentifyRequest>,
    statuses: HashMap<String, JobStatus>,
    worker_running: bool,
}

impl JobQueue {
    pub fn push(&mut self, req: IdentifyRequest) -> Result<(), String> {
        if let Some(existing) = self.statuses.get(&req.meeting_id) {
            return Err(existing.kind.queued_message().into());
        }
        self.statuses.insert(
            req.meeting_id.clone(),
            JobStatus {
                meeting_id: req.meeting_id.clone(),
                kind: req.kind.clone(),
                state: JobState::Queued,
                stage: None,
                percent: 0,
                message: "Waiting to start…".into(),
            },
        );
        self.queue.push_back(req);
        Ok(())
    }

    pub fn start_next(&mut self) -> Option<IdentifyRequest> {
        let req = self.queue.pop_front()?;
        if let Some(s) = self.statuses.get_mut(&req.meeting_id) {
            s.state = JobState::Running;
        }
        Some(req)
    }

    /// Remove a job that has not started yet.
    pub fn cancel_queued(&mut self, meeting_id: &str) -> Option<IdentifyRequest> {
        let index = self.queue.iter().position(|r| r.meeting_id == meeting_id)?;
        let req = self.queue.remove(index)?;
        self.statuses.remove(meeting_id);
        Some(req)
    }

    pub fn is_running(&self, meeting_id: &str) -> bool {
        matches!(self.statuses.get(meeting_id), Some(JobStatus { state: JobState::Running, .. }))
    }

    pub fn cancel(&mut self, meeting_id: &str) -> CancelOutcome {
        if let Some(req) = self.cancel_queued(meeting_id) {
            return CancelOutcome::Queued(req);
        }
        if self.is_running(meeting_id) {
            CancelOutcome::Running
        } else {
            CancelOutcome::NotFound
        }
    }

    /// Record progress. Returns None when nothing changed, so no event is sent.
    pub fn update(&mut self, meeting_id: &str, stage: &'static str, percent: u32, message: &str) -> Option<JobStatus> {
        let s = self.statuses.get_mut(meeting_id)?;
        if s.stage == Some(stage) && s.percent == percent && s.message == message {
            return None;
        }
        s.stage = Some(stage);
        s.percent = percent;
        s.message = message.to_string();
        Some(s.clone())
    }

    pub fn finish(&mut self, meeting_id: &str) {
        self.statuses.remove(meeting_id);
    }

    pub fn status(&self, meeting_id: &str) -> Option<JobStatus> {
        self.statuses.get(meeting_id).cloned()
    }
}

const IDENTIFYING_MESSAGE: &str = "Speaker identification is running for this meeting; try again when it finishes";
const NAMING_MESSAGE: &str = "Finding names for this meeting; try again when it finishes";
const RETRANSCRIBING_MESSAGE: &str = "This meeting is being retranscribed; try again when it finishes";

static JOBS: Lazy<Mutex<JobQueue>> = Lazy::new(|| Mutex::new(JobQueue::default()));
/// Cancel flag of the running job. Set and cleared only while holding the JOBS lock.
static CANCEL_CURRENT: AtomicBool = AtomicBool::new(false);
/// Meetings being retranscribed. Lock order everywhere: JOBS first, then this.
static BUSY_RETRANSCRIBING: Lazy<Mutex<HashSet<String>>> = Lazy::new(Default::default);
/// Only one diarization runs at a time, whether it comes from the Identify queue,
/// retranscription or import.
static DIARIZE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Serialises transcripts.json rewrites (jobs and speaker edits).
static JSON_REWRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn jobs() -> std::sync::MutexGuard<'static, JobQueue> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

fn busy_retranscribing() -> std::sync::MutexGuard<'static, HashSet<String>> {
    BUSY_RETRANSCRIBING.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_cancelled() -> bool {
    CANCEL_CURRENT.load(Ordering::SeqCst)
}

pub fn status(meeting_id: &str) -> Option<JobStatus> {
    jobs().status(meeting_id)
}

/// Refuse while identification is queued or running for the meeting, or while it is being
/// retranscribed: either would overwrite its speakers and rows.
pub fn ensure_idle(meeting_id: &str) -> Result<(), String> {
    ensure_idle_locked(&jobs(), meeting_id)
}

/// True while identification is queued or running for the meeting, or it is being retranscribed.
pub fn is_busy(meeting_id: &str) -> bool {
    ensure_idle(meeting_id).is_err()
}

/// `ensure_idle` for a caller that already holds the queue lock.
fn ensure_idle_locked(q: &JobQueue, meeting_id: &str) -> Result<(), String> {
    if let Some(status) = q.statuses.get(meeting_id) {
        return Err(status.kind.busy_message().into());
    }
    if is_retranscribing(meeting_id) {
        return Err(RETRANSCRIBING_MESSAGE.into());
    }
    Ok(())
}

/// Marks a meeting as being retranscribed until dropped.
pub struct RetranscribeClaim(String);

impl Drop for RetranscribeClaim {
    fn drop(&mut self) {
        busy_retranscribing().remove(&self.0);
    }
}

/// Atomically refuse when identification is queued or running for the meeting, or when the
/// meeting is already being retranscribed; otherwise mark it as being retranscribed so no
/// identification job can start on it.
pub fn claim_for_retranscription(meeting_id: &str) -> Result<RetranscribeClaim, String> {
    let q = jobs();
    if is_retranscribing(meeting_id) {
        return Err("This meeting is already being retranscribed".into());
    }
    ensure_idle_locked(&q, meeting_id)?;
    busy_retranscribing().insert(meeting_id.to_string());
    drop(q);
    Ok(RetranscribeClaim(meeting_id.to_string()))
}

pub fn is_retranscribing(meeting_id: &str) -> bool {
    busy_retranscribing().contains(meeting_id)
}

pub fn enqueue<R: Runtime>(app: &AppHandle<R>, req: IdentifyRequest) -> Result<(), String> {
    let meeting_id = req.meeting_id.clone();
    let (spawn_worker, queued) = {
        let mut q = jobs();
        // Not ensure_idle: a meeting that already has a job gets push's own message.
        if is_retranscribing(&meeting_id) {
            return Err(RETRANSCRIBING_MESSAGE.into());
        }
        q.push(req)?;
        let spawn = !q.worker_running;
        q.worker_running = true;
        (spawn, q.status(&meeting_id))
    };
    if let Some(s) = queued {
        let _ = app.emit(PROGRESS_EVENT, s);
    }
    if spawn_worker {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { worker(app).await });
    }
    Ok(())
}

pub fn cancel<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> Result<(), String> {
    let outcome = {
        let mut q = jobs();
        let outcome = q.cancel(meeting_id);
        if matches!(outcome, CancelOutcome::Running) {
            // Set under the queue lock: the worker clears the flag under the same lock before it
            // starts the next job, so this request cannot leak into another job.
            CANCEL_CURRENT.store(true, Ordering::SeqCst);
        }
        outcome
    };
    match outcome {
        CancelOutcome::Queued(req) => {
            let _ = app.emit(ERROR_EVENT, error_payload(&req, req.kind.cancelled_message(), true));
            Ok(())
        }
        CancelOutcome::Running => Ok(()),
        CancelOutcome::NotFound => Err("No speaker identification is running for this meeting".into()),
    }
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, stage: &'static str, percent: u32, message: &str) {
    let updated = jobs().update(meeting_id, stage, percent, message);
    if let Some(s) = updated {
        let _ = app.emit(PROGRESS_EVENT, s);
    }
}

/// Outcome of a finished job.
pub struct IdentifyOutcome {
    pub speaker_count: usize,
    /// Shown to the user when rows with two speakers had to keep their majority label.
    pub warning: Option<String>,
}

/// Names written by a naming job.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NamingOutcome {
    pub named: usize,
    pub suggested: usize,
}

/// What a finished job reports.
enum JobOutcome {
    Identify(IdentifyOutcome),
    Naming(NamingOutcome),
}

fn complete_payload(req: &IdentifyRequest, outcome: &JobOutcome) -> serde_json::Value {
    let (speaker_count, warning, named, suggested) = match outcome {
        JobOutcome::Identify(o) => (o.speaker_count, o.warning.clone(), 0, 0),
        JobOutcome::Naming(o) => (0, None, o.named, o.suggested),
    };
    serde_json::json!({
        "meeting_id": req.meeting_id,
        "kind": req.kind,
        "speaker_count": speaker_count,
        "automatic": req.automatic,
        "warning": warning,
        "named": named,
        "suggested": suggested
    })
}

fn error_payload(req: &IdentifyRequest, error: &str, cancelled: bool) -> serde_json::Value {
    serde_json::json!({
        "meeting_id": req.meeting_id,
        "kind": req.kind,
        "error": error,
        "automatic": req.automatic,
        "cancelled": cancelled
    })
}

async fn run_job<R: Runtime>(app: &AppHandle<R>, req: &IdentifyRequest) -> Result<JobOutcome> {
    match &req.kind {
        JobKind::Identify { folder_path, num_speakers } => {
            run_identify(app, req, folder_path, *num_speakers).await.map(JobOutcome::Identify)
        }
        JobKind::Naming { allow_cloud } => run_naming(app, req, *allow_cloud).await.map(JobOutcome::Naming),
    }
}

async fn worker<R: Runtime>(app: AppHandle<R>) {
    loop {
        let next = {
            let mut q = jobs();
            let next = q.start_next();
            if next.is_none() {
                q.worker_running = false;
            } else {
                CANCEL_CURRENT.store(false, Ordering::SeqCst);
            }
            next
        };
        let Some(req) = next else { break };

        // Run each job in its own task: a panic comes back as an error and the queue keeps going.
        let (job_app, job_req) = (app.clone(), req.clone());
        let result = match tauri::async_runtime::spawn(async move { run_job(&job_app, &job_req).await }).await {
            Ok(result) => result,
            Err(e) => Err(anyhow!("Speaker job task panicked: {}", e)),
        };
        jobs().finish(&req.meeting_id);

        match result {
            Ok(outcome) => {
                match &outcome {
                    JobOutcome::Identify(o) => {
                        log::info!("Speaker identification finished for {}: {} speakers", req.meeting_id, o.speaker_count)
                    }
                    JobOutcome::Naming(o) => {
                        log::info!("Finding names finished for {}: {} named, {} suggested", req.meeting_id, o.named, o.suggested)
                    }
                }
                let _ = app.emit(COMPLETE_EVENT, complete_payload(&req, &outcome));
            }
            Err(e) => {
                let cancelled = e.is::<Cancelled>();
                if cancelled {
                    log::info!("{} job cancelled for {}", req.kind.name(), req.meeting_id);
                } else {
                    log::warn!("{} job failed for {}: {:#}", req.kind.name(), req.meeting_id, e);
                }
                let _ = app.emit(ERROR_EVENT, error_payload(&req, &format!("{e:#}"), cancelled));
            }
        }
    }
}

/// Find the meeting's audio file, waiting up to `timeout` for it to appear and stop growing.
pub async fn wait_for_audio(folder: &Path, timeout: Duration, cancelled: &(dyn Fn() -> bool + Sync)) -> Result<PathBuf> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_size: Option<u64> = None;
    loop {
        if cancelled() {
            return Err(Cancelled.into());
        }
        if let Ok(path) = crate::audio::retranscription::find_audio_file(folder) {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if timeout.is_zero() || (size > 0 && last_size == Some(size)) {
                return Ok(path);
            }
            last_size = Some(size);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("No audio file found in {}", folder.display()));
        }
        tokio::time::sleep(LOCK_POLL).await;
    }
}

/// Wait while `recording()` reports a live recording, calling `waiting` once if it has to wait.
pub async fn wait_while_recording<F, Fut>(
    recording: F,
    cancelled: &(dyn Fn() -> bool + Sync),
    waiting: impl FnOnce(),
    poll: Duration,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let mut waiting = Some(waiting);
    while recording().await {
        if cancelled() {
            return Err(Cancelled.into());
        }
        if let Some(w) = waiting.take() {
            w();
        }
        tokio::time::sleep(poll).await;
    }
    Ok(())
}

/// Hold an automatic job before a heavy stage while a recording is live, so identification does
/// not compete with live transcription for CPU. Jobs the user started are not held.
async fn yield_to_recording<R: Runtime>(app: &AppHandle<R>, req: &IdentifyRequest, percent: u32) -> Result<()> {
    if !req.automatic {
        return Ok(());
    }
    wait_while_recording(
        crate::audio::recording_commands::is_recording,
        &is_cancelled,
        || emit_progress(app, &req.meeting_id, "waiting", percent, "Waiting for the recording to finish…"),
        LOCK_POLL,
    )
    .await
}

/// 16 kHz samples plus what the time map needs to know about the decoded file.
struct DecodedMeetingAudio {
    samples: Arc<Vec<f32>>,
    native_rate: u32,
    native_frames: usize,
}

async fn decode_16k(path: PathBuf) -> Result<DecodedMeetingAudio> {
    tokio::task::spawn_blocking(move || -> Result<DecodedMeetingAudio> {
        let decoded = decode_audio_file(&path)?;
        let native_rate = decoded.sample_rate;
        let native_frames = decoded.samples.len() / decoded.channels.max(1) as usize;
        // Consumes the decoded samples instead of copying them (long meetings are large).
        let samples = Arc::new(decoded.into_whisper_format());
        Ok(DecodedMeetingAudio { samples, native_rate, native_frames })
    })
    .await
    .map_err(|e| anyhow!("Decode task panicked: {}", e))?
}

/// Wait for the guard `lock()` resolves to, giving up with `Cancelled` once `cancelled` returns true.
async fn lock_cancellable<G, Fut>(lock: impl Fn() -> Fut, cancelled: &(dyn Fn() -> bool + Sync)) -> Result<G>
where
    Fut: std::future::Future<Output = G>,
{
    loop {
        if cancelled() {
            return Err(Cancelled.into());
        }
        if let Ok(guard) = tokio::time::timeout(LOCK_POLL, lock()).await {
            return Ok(guard);
        }
    }
}

/// Download models if needed, then diarize on a blocking thread. Only one diarization runs at a
/// time; `waiting` is called once when another one is running. `cancelled` is checked while
/// waiting, during the download and during diarization.
pub async fn diarize_samples(
    samples: Arc<Vec<f32>>,
    num_speakers: Option<usize>,
    on_download: impl Fn(DownloadProgress) + Send + Sync + 'static,
    waiting: impl Fn() + Send + 'static,
    progress: impl Fn(u32) + Send + 'static,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<Diarization> {
    let cancelled = Arc::new(cancelled);
    let _guard = match DIARIZE_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            waiting();
            lock_cancellable(|| DIARIZE_LOCK.lock(), &*cancelled).await?
        }
    };
    let dir = models::models_directory()?;
    if !models::status(&dir).installed {
        let c = cancelled.clone();
        models::ensure_models(&dir, on_download, move || (*c)()).await?;
    }
    if (*cancelled)() {
        return Err(Cancelled.into());
    }
    let c = cancelled.clone();
    tokio::task::spawn_blocking(move || {
        let mut diarizer = Diarizer::load(&dir)?;
        let opts = DiarizeOptions { num_speakers, ..Default::default() };
        let mut report = |p: u32| progress(p);
        diarizer.diarize(&samples, &opts, &mut report, &|| (*c)())
    })
    .await
    .map_err(|e| anyhow!("Diarization task panicked: {}", e))?
}

/// Speaker identification inside an import or retranscription. Returns the diarization, or None
/// with a warning for the user when no speakers were found or identification failed (the job then
/// continues without speakers). `report` shows a progress message; Err when `cancelled` returned true.
pub async fn diarize_for_batch<R: Runtime>(
    app: &AppHandle<R>,
    samples: Arc<Vec<f32>>,
    num_speakers: Option<usize>,
    report: impl Fn(&str) + Clone + Send + Sync + 'static,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<(Option<Diarization>, Option<String>), Cancelled> {
    report("Identifying speakers...");
    let (app, on_download, on_wait, on_progress) = (app.clone(), report.clone(), report.clone(), report);
    let result = diarize_samples(
        samples,
        num_speakers,
        move |p| {
            super::commands::emit_download_progress(&app, p.clone());
            on_download(&format!("Downloading speaker models... {}%", p.percent));
        },
        move || on_wait("Waiting for another speaker identification..."),
        move |p| on_progress(&format!("Identifying speakers... {}%", p)),
        cancelled,
    )
    .await;
    match result {
        Ok(d) if !d.speakers.is_empty() => Ok((Some(d), None)),
        Ok(_) => Ok((None, Some("No distinct speakers were found".to_string()))),
        Err(e) if e.is::<Cancelled>() => Err(Cancelled),
        Err(e) => {
            log::warn!("Speaker identification failed, continuing without speakers: {:#}", e);
            Ok((None, Some(format!("Speaker identification failed: {:#}", e))))
        }
    }
}

/// New speakers for `d`. Each takes the name, person link and suggestion of the previous speaker
/// with the same voice, and the meeting's rejections follow their voices to the new keys (a
/// rejection whose voice is gone is dropped). Then, with `remember_voices`, the remaining
/// speakers are matched against people named in other meetings. Reads and writes through
/// `conn`, so call it inside the write transaction.
pub async fn speaker_write_names(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    d: &Diarization,
    remember_voices: bool,
) -> Result<Vec<NewSpeaker>> {
    let previous: Vec<(String, Vec<f32>, Option<String>, SpeakerLink)> = SpeakersRepository::list_conn(&mut *conn, meeting_id)
        .await?
        .into_iter()
        .filter_map(|s| s.embedding.map(|e| (s.speaker_key, e, s.display_name, s.link)))
        .collect();
    let old: Vec<Vec<f32>> = previous.iter().map(|p| p.1.clone()).collect();
    let new: Vec<(String, Vec<f32>)> = d.speakers.iter().map(|s| (s.key.clone(), s.embedding.clone())).collect();
    let matched = carry_over(&new, &old, CARRY_OVER_MIN_SIMILARITY);

    let new_key_of: HashMap<&str, &str> =
        matched.iter().map(|(new_key, &i)| (previous[i].0.as_str(), new_key.as_str())).collect();
    let rejections = PeopleRepository::rejections_conn(&mut *conn, meeting_id).await?;
    PeopleRepository::clear_rejections_conn(&mut *conn, meeting_id).await?;
    for (old_key, person_id) in &rejections {
        if let Some(new_key) = new_key_of.get(old_key.as_str()) {
            PeopleRepository::add_rejection_conn(&mut *conn, meeting_id, new_key, person_id).await?;
        }
    }

    let mut speakers: Vec<NewSpeaker> = d
        .speakers
        .iter()
        .map(|s| match matched.get(&s.key) {
            Some(&i) => NewSpeaker { display_name: previous[i].2.clone(), link: previous[i].3.clone(), ..s.into() },
            None => s.into(),
        })
        .collect();
    let (linked, suggested) =
        super::people::match_new_speakers_conn(&mut *conn, meeting_id, &mut speakers, remember_voices).await?;
    if linked + suggested > 0 {
        log::info!("Voice matching for {}: {} linked, {} suggested", meeting_id, linked, suggested);
    }
    Ok(speakers)
}

/// Rewrite transcripts.json from the database into the meeting's stored folder, or `fallback`
/// when it has none. Failures are logged; the database stays the source of truth.
pub async fn rewrite_transcripts_json(pool: &SqlitePool, meeting_id: &str, fallback: Option<&Path>) {
    let Some(folder) = transcripts_json_folder(pool, meeting_id, fallback).await else {
        return;
    };
    if let Err(e) = write_json_from_db(pool, meeting_id, folder).await {
        log::warn!("Failed to rewrite transcripts.json for {}: {:#}", meeting_id, e);
    }
}

/// `rewrite_transcripts_json` for each meeting in a background task, so a command returns as soon
/// as its change is committed.
pub fn rewrite_transcripts_json_later(pool: &SqlitePool, meeting_ids: impl IntoIterator<Item = String>) {
    let meeting_ids: Vec<String> = meeting_ids.into_iter().collect();
    if meeting_ids.is_empty() {
        return;
    }
    let pool = pool.clone();
    tauri::async_runtime::spawn(async move {
        for meeting_id in &meeting_ids {
            rewrite_transcripts_json(&pool, meeting_id, None).await;
        }
    });
}

/// The meeting's rows in transcript order.
async fn transcripts_in_order(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Transcript>, sqlx::Error> {
    sqlx::query_as::<_, Transcript>("SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time")
        .bind(meeting_id)
        .fetch_all(pool)
        .await
}

async fn write_json_from_db(pool: &SqlitePool, meeting_id: &str, folder: PathBuf) -> Result<()> {
    // Held across the read and the write, so concurrent rewrites cannot interleave or write stale data.
    let _guard = JSON_REWRITE_LOCK.lock().await;
    let segments: Vec<TranscriptSegment> =
        transcripts_in_order(pool, meeting_id).await?.into_iter().map(TranscriptSegment::from).collect();
    let labels = SpeakersRepository::labels(pool, meeting_id).await?;
    tokio::task::spawn_blocking(move || write_transcripts_json(&folder, &segments, &labels))
        .await
        .map_err(|e| anyhow!("transcripts.json write task panicked: {}", e))?
}

/// The meeting's folder as stored in the database; `fallback` when it has none.
async fn transcripts_json_folder(pool: &SqlitePool, meeting_id: &str, fallback: Option<&Path>) -> Option<PathBuf> {
    match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
        Ok(Some(meeting)) => meeting.folder_path.filter(|f| !f.is_empty()).map(PathBuf::from),
        Ok(None) => None,
        Err(e) => {
            log::warn!("Failed to read the folder of meeting {}: {}", meeting_id, e);
            None
        }
    }
    .or_else(|| fallback.map(Path::to_path_buf))
}

/// Resolves once the running job is cancelled.
async fn until_cancelled() {
    while !is_cancelled() {
        tokio::time::sleep(LOCK_POLL).await;
    }
}

async fn run_naming<R: Runtime>(app: &AppHandle<R>, req: &IdentifyRequest, allow_cloud: bool) -> Result<NamingOutcome> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?
        .db_manager
        .pool()
        .clone();
    emit_progress(app, &req.meeting_id, "naming", 10, "Finding names…");
    let model = summary_model_from_settings(&pool, app.path().app_data_dir().ok()).await.map_err(|e| anyhow!(e))?;
    // Checked again here: the saved model may have changed while the job waited.
    ensure_may_send(req.automatic, allow_cloud, model.is_local())?;
    let outcome = name_from_conversation(&pool, &req.meeting_id, &model, until_cancelled()).await?;
    emit_progress(app, &req.meeting_id, "done", 100, "Done");
    Ok(outcome)
}

/// Err when an automatic naming job would send the transcript to a model that is not local
/// without the user's consent (`allow_cloud`). Jobs the user started always run.
fn ensure_may_send(automatic: bool, allow_cloud: bool, local: bool) -> Result<()> {
    if !automatic || allow_cloud || local {
        return Ok(());
    }
    log::info!("Skipping the automatic name guess: the summary model is not local");
    Err(anyhow!("The summary model changed, so names were not guessed automatically. Use Guess names to run it."))
}

/// The naming job to queue, or None when an automatic request is declined because the saved
/// summary model is not local and `allow_cloud` is not set.
pub async fn naming_request(
    pool: &SqlitePool,
    meeting_id: String,
    automatic: bool,
    allow_cloud: Option<bool>,
) -> Option<IdentifyRequest> {
    let allow_cloud = allow_cloud == Some(true);
    if automatic && !allow_cloud && !saved_model_is_local(pool).await {
        return None;
    }
    Some(IdentifyRequest { meeting_id, automatic, kind: JobKind::Naming { allow_cloud } })
}

/// The meeting's summary markdown, when a summary was generated.
async fn meeting_summary_markdown(pool: &SqlitePool, meeting_id: &str) -> Option<String> {
    match SummaryProcessesRepository::get_summary_data_for_meeting(pool, meeting_id).await {
        Ok(process) => process?
            .result
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("markdown").and_then(|m| m.as_str()).map(str::to_string))
            .filter(|m| !m.trim().is_empty()),
        Err(e) => {
            log::warn!("Failed to read the summary of {}: {}", meeting_id, e);
            None
        }
    }
}

/// Asks `model` for the names said in the meeting and writes what passes the checks: applied
/// names and suggestions, only on speakers that are still unnamed. No connection is held while
/// the model answers; the write is one transaction. Only the model call races `cancelled`: once
/// the answer is in, the write and the transcripts.json rewrite always finish together.
pub async fn name_from_conversation(
    pool: &SqlitePool,
    meeting_id: &str,
    model: &dyn NamingModel,
    cancelled: impl Future<Output = ()>,
) -> Result<NamingOutcome> {
    let speakers = SpeakersRepository::list(pool, meeting_id).await?;
    if speakers.is_empty() {
        return Err(anyhow!("Identify speakers first"));
    }
    if speakers.iter().all(|s| s.display_name.is_some()) {
        return Ok(NamingOutcome::default());
    }
    let lines: Vec<NamingLine> = transcripts_in_order(pool, meeting_id)
        .await?
        .into_iter()
        .filter_map(|r| {
            let speaker = r.speaker?;
            Some(NamingLine { start_s: r.audio_start_time.unwrap_or(0.0), speaker, text: r.transcript })
        })
        .collect();
    if lines.is_empty() {
        return Ok(NamingOutcome::default());
    }
    let summary = meeting_summary_markdown(pool, meeting_id).await;
    let (people, rejected_names) = {
        let mut conn = pool.acquire().await?;
        // Rejection-only people are not offered: they are names the user said a speaker is not.
        let people = PeopleRepository::linked_conn(&mut conn).await?;
        let rejected_names = PeopleRepository::rejected_names_conn(&mut conn, meeting_id)
            .await?
            .into_iter()
            .map(|(key, name)| (key, name_key(&name)))
            .collect();
        (people, rejected_names)
    };
    let input = NamingInput {
        lines,
        summary,
        speakers: speakers
            .into_iter()
            .map(|s| NamingSpeaker {
                has_voice_suggestion: s.link.suggestion_source == Some(SuggestionSource::Voice),
                key: s.speaker_key,
                display_name: s.display_name,
            })
            .collect(),
        people: people.into_iter().map(|p| KnownPerson { id: p.id, name: p.name }).collect(),
        rejected_names,
    };
    let decisions = tokio::select! {
        biased;
        _ = cancelled => return Err(Cancelled.into()),
        result = propose_names(model, &input) => result.map_err(|e| anyhow!(e))?,
    };

    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    let (named, suggested) = SpeakersRepository::apply_naming_conn(&mut tx, meeting_id, &decisions).await?;
    tx.commit().await?;
    drop(conn);
    if named > 0 {
        rewrite_transcripts_json(pool, meeting_id, None).await;
    }
    Ok(NamingOutcome { named, suggested })
}

struct StoredRow {
    id: String,
    text: String,
    span: Option<RowSpan>,
}

/// Labels and splits produced for the stored rows.
#[derive(Default)]
struct RowUpdates {
    row_labels: Vec<(String, Option<String>)>,
    row_splits: Vec<(String, Vec<SplitRow>)>,
    pieces_done: usize,
    /// Rows with a speaker change whose existing text was split.
    text_splits: usize,
    /// Rows with a speaker change that kept their majority label because they could not be cut.
    kept_whole: usize,
    /// Every row labelled whole although it holds a speaker change.
    mixed_rows: Vec<String>,
}

impl RowUpdates {
    /// A row with a speaker change that stays one row: majority label, marked mixed.
    fn label_mixed_row(&mut self, row_id: &str, majority: String) {
        self.row_labels.push((row_id.to_string(), Some(majority)));
        self.mixed_rows.push(row_id.to_string());
    }
}

/// The 16 kHz sample range of each piece of a row with a speaker change. Pieces come from
/// `label_rows`, so there are at least two and none is shorter than MIN_TRANSCRIBED_PIECE_S.
/// None when a piece has no audio.
fn piece_ranges(pieces: &[Turn], time_map: TimeMap, samples_len: usize) -> Option<Vec<Range<usize>>> {
    pieces
        .iter()
        .map(|p| {
            // Pieces are in transcript time; cut the audio at the matching file positions.
            let a = ((time_map.file_s(p.start_s) * 16000.0) as usize).min(samples_len);
            let b = ((time_map.file_s(p.end_s) * 16000.0) as usize).min(samples_len);
            (b > a).then_some(a..b)
        })
        .collect()
}

/// The rows replacing a row cut into `pieces`, when every piece has text; otherwise None, since
/// the row's words would be lost.
fn split_rows(pieces: &[Turn], texts: &[String]) -> Option<Vec<SplitRow>> {
    if texts.len() != pieces.len() || texts.iter().any(|t| t.trim().is_empty()) {
        return None;
    }
    Some(
        pieces
            .iter()
            .zip(texts)
            .map(|(p, t)| SplitRow { text: t.trim().to_string(), start_s: p.start_s, end_s: p.end_s, speaker: p.key.clone() })
            .collect(),
    )
}

/// Re-transcribing the pieces of rows with a speaker change costs one engine call per piece, which
/// is only quick on Parakeet or on Whisper with a GPU backend. Otherwise the existing text is split.
fn engine_is_fast(provider: Option<&str>, whisper_backend: WhisperCompiledBackend) -> bool {
    provider == Some("parakeet") || whisper_backend != WhisperCompiledBackend::Cpu
}

/// Splits a row's existing text among its pieces; None when the text cannot be cut.
fn split_row_text(text: &str, span: RowSpan, pieces: &[Turn]) -> Option<Vec<SplitRow>> {
    split_rows(pieces, &split_text_at_turns(text, span, pieces)?)
}

/// How rows with a speaker change are split.
enum Splitter<'a> {
    /// The recording's timing does not match the transcript: keep rows whole.
    KeepWhole,
    /// Split each row's existing text.
    Text,
    /// Re-transcribe each piece; rows whose pieces fail fall back to splitting their text.
    Engine(&'a BatchEngine),
}

async fn meeting_exists(conn: &mut SqliteConnection, meeting_id: &str) -> Result<bool> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(found.is_some())
}

async fn run_identify<R: Runtime>(
    app: &AppHandle<R>,
    req: &IdentifyRequest,
    folder_path: &Path,
    num_speakers: Option<usize>,
) -> Result<IdentifyOutcome> {
    let started = Instant::now();
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?
        .db_manager
        .pool()
        .clone();
    let meeting_id = req.meeting_id.clone();

    emit_progress(app, &meeting_id, "audio", 0, "Finding recording…");
    let wait = if req.automatic { AUTO_AUDIO_WAIT } else { Duration::ZERO };
    let audio_path = wait_for_audio(folder_path, wait, &is_cancelled).await?;

    yield_to_recording(app, req, 1).await?;
    emit_progress(app, &meeting_id, "audio", 1, "Decoding audio…");
    let decoded = decode_16k(audio_path).await?;
    let samples = decoded.samples;
    let t_decode = started.elapsed();
    if is_cancelled() {
        return Err(Cancelled.into());
    }
    {
        let mut conn = pool.acquire().await?;
        if !meeting_exists(&mut conn, &meeting_id).await? {
            // Deleted while the job waited: stop before diarizing.
            return Err(Cancelled.into());
        }
    }
    let time_map = recording_time_map(read_metadata(folder_path).as_ref(), decoded.native_rate, decoded.native_frames);

    let rows: Vec<StoredRow> = transcripts_in_order(&pool, &meeting_id)
        .await?
        .into_iter()
        .map(|r| StoredRow {
            span: match (r.audio_start_time, r.audio_end_time) {
                (Some(a), Some(b)) if b > a => Some(RowSpan { start_s: a, end_s: b }),
                _ => None,
            },
            id: r.id,
            text: r.transcript,
        })
        .collect();

    yield_to_recording(app, req, 2).await?;
    let diarize_started = Instant::now();
    let (app_dl, app_wait, app_p) = (app.clone(), app.clone(), app.clone());
    let (id_dl, id_wait, id_p) = (meeting_id.clone(), meeting_id.clone(), meeting_id.clone());
    let diarization = diarize_samples(
        samples.clone(),
        num_speakers,
        move |p| {
            super::commands::emit_download_progress(&app_dl, p.clone());
            emit_progress(&app_dl, &id_dl, "download", 2, &format!("Downloading speaker models… {}%", p.percent));
        },
        move || emit_progress(&app_wait, &id_wait, "waiting", 2, "Waiting for another speaker identification…"),
        move |p| {
            let stage = if p < 40 { "segmentation" } else if p < 90 { "embeddings" } else { "clustering" };
            emit_progress(&app_p, &id_p, stage, 5 + p * 80 / 100, "Identifying speakers…");
        },
        is_cancelled,
    )
    .await?;
    let t_diarize = diarize_started.elapsed();
    if diarization.speakers.is_empty() {
        return Err(anyhow!("No speech found in the recording"));
    }

    // Turns are in decoded-file time; rows and stored split pieces use the transcript clock.
    let clock_turns: Vec<Turn> = diarization
        .turns
        .iter()
        .map(|t| Turn { start_s: time_map.clock_s(t.start_s), end_s: time_map.clock_s(t.end_s), key: t.key.clone() })
        .collect();
    let spans: Vec<Option<RowSpan>> = rows.iter().map(|r| r.span).collect();
    let labels = label_rows(&spans, &clock_turns);
    let mixed_count = labels.iter().filter(|l| matches!(l, RowLabel::Mixed { .. })).count();

    // Never cut rows when the timing cannot be trusted: a wrong cut replaces text with other audio.
    let decoded_s = samples.len() as f64 / 16_000.0;
    let rows_end = rows.iter().filter_map(|r| r.span.map(|s| s.end_s)).fold(0.0, f64::max);
    let timing_ok = time_map.allows_splitting() && time_map.file_s(rows_end) <= decoded_s + 1.0;
    let mut split_warning: Option<String> = None;
    if mixed_count > 0 && !timing_ok {
        log::warn!(
            "Recording timing does not match the transcript for {} ({:?}, rows end at {:.1}s, audio {:.1}s); keeping majority labels",
            meeting_id, time_map, rows_end, decoded_s
        );
        split_warning = Some("Lines with two speakers were kept whole: the recording's timing could not be matched to the transcript".into());
    }

    let load_started = Instant::now();
    let mut batch_guard: Option<tokio::sync::OwnedMutexGuard<()>> = None;
    let provider: Option<String> = sqlx::query_scalar("SELECT provider FROM transcript_settings WHERE id = '1'")
        .fetch_optional(&pool)
        .await
        .unwrap_or_else(|e| {
            log::warn!("Failed to read the transcription provider: {}", e);
            None
        });
    let use_engine = engine_is_fast(provider.as_deref(), WhisperCompiledBackend::current());
    let engine = if mixed_count > 0 && timing_ok && use_engine {
        yield_to_recording(app, req, 86).await?;
        if batch_engine_busy() {
            emit_progress(app, &meeting_id, "waiting", 86, "Waiting for another transcription to finish…");
        }
        // Held from engine load to unload, so another batch job cannot unload or swap the model.
        batch_guard = Some(lock_cancellable(acquire_batch_engine_lock, &is_cancelled).await?);
        emit_progress(app, &meeting_id, "splitting", 86, "Loading transcription engine…");
        match crate::audio::retranscription::load_configured_engine(app, provider.as_deref()).await {
            Ok(engine) => Some(engine),
            Err(e) => {
                log::warn!("Cannot re-transcribe rows with speaker changes, splitting their text instead: {:#}", e);
                None
            }
        }
    } else {
        None
    };
    let splitter = match &engine {
        _ if !timing_ok => Splitter::KeepWhole,
        Some(engine) => Splitter::Engine(engine),
        None => Splitter::Text,
    };
    let t_load = load_started.elapsed();

    let split_started = Instant::now();
    // Same language and translate setting as live transcription, so split text matches its rows.
    let language = crate::get_language_preference_internal();
    let updates = build_row_updates(app, &meeting_id, &rows, labels, &samples, time_map, splitter, mixed_count, language).await;
    // Unload on every exit from the split phase (success, piece failure or cancel).
    if let Some(engine) = &engine {
        unload_engine_after_batch(engine.is_parakeet()).await;
    }
    drop(batch_guard);
    let RowUpdates { row_labels, row_splits, pieces_done, text_splits, kept_whole, mixed_rows } = updates?;
    let t_split = split_started.elapsed();
    if is_cancelled() {
        return Err(Cancelled.into());
    }

    emit_progress(app, &meeting_id, "saving", 97, "Saving speakers…");
    let save_started = Instant::now();
    // Read before the transaction: the preference lives in the settings store, not the database.
    let remember_voices = crate::audio::recording_preferences::remember_voices(app).await;
    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    if !meeting_exists(&mut tx, &meeting_id).await? {
        // Deleted while the job ran; dropping the transaction rolls it back.
        return Err(Cancelled.into());
    }
    // Previous names are read inside the transaction, so a rename committed meanwhile is kept.
    let speakers = speaker_write_names(&mut tx, &meeting_id, &diarization, remember_voices).await?;
    let speaker_count = speakers.len();
    SpeakersRepository::replace_for_meeting(&mut tx, &meeting_id, &SpeakerWrite { speakers, row_labels, row_splits, mixed_rows }).await?;
    tx.commit().await?;
    drop(conn);
    rewrite_transcripts_json(&pool, &meeting_id, Some(folder_path)).await;
    let t_save = save_started.elapsed();

    let total = started.elapsed();
    log::info!(
        "Identify job {}: {:.1}s audio | decode {:?} | diarize {:?} | engine load {:?} | split {} rows ({} by text)/{} pieces ({} kept whole) {:?} | save {:?} | total {:?} ({:.2}% of audio)",
        meeting_id,
        decoded_s,
        t_decode,
        t_diarize,
        t_load,
        mixed_count,
        text_splits,
        pieces_done,
        kept_whole,
        t_split,
        t_save,
        total,
        total.as_secs_f64() / decoded_s.max(1e-9) * 100.0
    );
    emit_progress(app, &meeting_id, "done", 100, "Done");
    let piece_warning = (kept_whole > 0).then(|| "Some lines with two speakers were kept whole".to_string());
    Ok(IdentifyOutcome { speaker_count, warning: split_warning.or(piece_warning) })
}

/// Label every row and split rows with a clear speaker change (see `Splitter`). A row that cannot
/// be cut keeps its text and the majority label.
#[allow(clippy::too_many_arguments)]
async fn build_row_updates<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    rows: &[StoredRow],
    labels: Vec<RowLabel>,
    samples: &[f32],
    time_map: TimeMap,
    splitter: Splitter<'_>,
    mixed_count: usize,
    language: Option<String>,
) -> Result<RowUpdates> {
    let mut out = RowUpdates::default();
    let mut done_mixed = 0usize;
    for (row, label) in rows.iter().zip(labels) {
        if is_cancelled() {
            return Err(Cancelled.into());
        }
        match label {
            RowLabel::Unlabeled => out.row_labels.push((row.id.clone(), None)),
            RowLabel::Single(k) => out.row_labels.push((row.id.clone(), Some(k))),
            RowLabel::Mixed { majority, pieces } => {
                done_mixed += 1;
                emit_progress(
                    app,
                    meeting_id,
                    "splitting",
                    86 + (done_mixed * 10 / mixed_count.max(1)) as u32,
                    "Splitting lines with speaker changes…",
                );
                let Some(span) = row.span.filter(|_| !matches!(splitter, Splitter::KeepWhole)) else {
                    out.label_mixed_row(&row.id, majority);
                    continue;
                };
                let transcribed = match splitter {
                    Splitter::Engine(engine) => {
                        transcribe_row_pieces(engine, &row.id, &pieces, time_map, samples, &language, &mut out.pieces_done).await
                    }
                    _ => None,
                };
                let split = transcribed.or_else(|| {
                    out.text_splits += 1;
                    split_row_text(&row.text, span, &pieces)
                });
                match split {
                    Some(split) => out.row_splits.push((row.id.clone(), split)),
                    None => {
                        out.kept_whole += 1;
                        out.label_mixed_row(&row.id, majority);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Re-transcribes each piece of a row with a speaker change. None when a piece has no audio, or
/// fails or comes back empty.
async fn transcribe_row_pieces(
    engine: &BatchEngine,
    row_id: &str,
    pieces: &[Turn],
    time_map: TimeMap,
    samples: &[f32],
    language: &Option<String>,
    pieces_done: &mut usize,
) -> Option<Vec<SplitRow>> {
    let ranges = piece_ranges(pieces, time_map, samples.len())?;
    let mut texts = Vec::with_capacity(ranges.len());
    for range in ranges {
        match engine.transcribe(samples[range].to_vec(), language.clone()).await {
            Ok(t) => texts.push(t),
            Err(e) => {
                log::warn!("Piece transcription failed for row {}, splitting its text instead: {:#}", row_id, e);
                break;
            }
        }
        *pieces_done += 1;
        if texts.last().is_some_and(|t| t.trim().is_empty()) {
            log::warn!("A piece of row {} transcribed to no text, splitting its text instead", row_id);
            break;
        }
    }
    split_rows(pieces, &texts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str) -> IdentifyRequest {
        IdentifyRequest {
            meeting_id: id.into(),
            automatic: false,
            kind: JobKind::Identify { folder_path: PathBuf::from("/tmp"), num_speakers: None },
        }
    }

    fn naming(id: &str) -> IdentifyRequest {
        IdentifyRequest { kind: JobKind::Naming { allow_cloud: false }, ..req(id) }
    }

    #[test]
    fn job_queue_rejects_duplicate_meeting() {
        let mut q = JobQueue::default();
        q.push(req("a")).unwrap();
        assert!(q.push(req("a")).is_err());
        q.push(req("b")).unwrap();
        assert_eq!(q.status("a").unwrap().state, JobState::Queued);
        assert_eq!(q.status("a").unwrap().stage, None);
        assert_eq!(q.start_next().unwrap().meeting_id, "a");
        assert_eq!(q.status("a").unwrap().state, JobState::Running);
        assert!(q.push(req("a")).is_err(), "running meeting must reject a second job");
        q.finish("a");
        q.push(req("a")).unwrap();
    }

    #[test]
    fn job_queue_runs_in_order_and_cancels_queued_jobs() {
        let mut q = JobQueue::default();
        q.push(req("a")).unwrap();
        q.push(req("b")).unwrap();
        q.push(req("c")).unwrap();
        assert_eq!(q.cancel_queued("b").map(|r| r.meeting_id), Some("b".to_string()));
        let first = q.start_next().unwrap();
        assert_eq!(first.meeting_id, "a");
        assert_eq!(q.status("a").unwrap().state, JobState::Running);
        q.finish("a");
        assert_eq!(q.start_next().unwrap().meeting_id, "c");
        assert!(q.status("b").is_none());
        q.finish("c");
        assert!(q.start_next().is_none());
    }

    #[test]
    fn cancel_reports_queued_running_and_unknown_jobs() {
        let mut q = JobQueue::default();
        q.push(IdentifyRequest { automatic: true, ..req("a") }).unwrap();
        q.push(req("b")).unwrap();
        assert_eq!(q.start_next().unwrap().meeting_id, "a");
        assert!(matches!(q.cancel("a"), CancelOutcome::Running));
        assert!(q.status("a").is_some(), "a running job stays visible until it stops");
        match q.cancel("b") {
            CancelOutcome::Queued(r) => assert!(!r.automatic),
            other => panic!("expected a queued job, got {other:?}"),
        }
        assert!(q.status("b").is_none());
        assert!(matches!(q.cancel("zzz"), CancelOutcome::NotFound));
    }

    #[test]
    fn progress_updates_without_changes_send_no_event() {
        let mut q = JobQueue::default();
        q.push(req("a")).unwrap();
        assert!(q.update("a", "segmentation", 10, "x").is_some());
        assert!(q.update("a", "segmentation", 10, "x").is_none());
        assert!(q.update("a", "segmentation", 11, "x").is_some());
        let s = q.update("a", "embeddings", 11, "x").unwrap();
        assert_eq!(s.stage, Some("embeddings"));
        assert!(q.update("missing", "segmentation", 1, "x").is_none());
    }

    #[test]
    fn ensure_idle_refuses_queued_and_running_jobs() {
        let mut q = JobQueue::default();
        assert!(ensure_idle_locked(&q, "a").is_ok());
        q.push(req("a")).unwrap();
        assert_eq!(ensure_idle_locked(&q, "a"), Err(IDENTIFYING_MESSAGE.to_string()));
        q.start_next();
        assert!(ensure_idle_locked(&q, "a").is_err());
        q.finish("a");
        assert!(ensure_idle_locked(&q, "a").is_ok());
    }

    #[test]
    fn retranscription_claim_is_released_on_drop() {
        let id = "claim-test-meeting";
        let claim = claim_for_retranscription(id).unwrap();
        assert!(is_retranscribing(id));
        assert_eq!(ensure_idle(id), Err(RETRANSCRIBING_MESSAGE.to_string()));
        // A second claim on the same meeting is refused and must not release the first one.
        assert!(claim_for_retranscription(id).is_err());
        assert!(is_retranscribing(id));
        drop(claim);
        assert!(!is_retranscribing(id));
        assert!(ensure_idle(id).is_ok());
    }

    fn turn(s: f64, e: f64, k: &str) -> Turn {
        Turn { start_s: s, end_s: e, key: k.into() }
    }

    fn split_row(text: &str, s: f64, e: f64, k: &str) -> SplitRow {
        SplitRow { text: text.into(), start_s: s, end_s: e, speaker: k.into() }
    }

    #[test]
    fn row_with_an_empty_piece_is_not_split() {
        let pieces = vec![turn(0.0, 2.0, "spk_0"), turn(2.0, 4.5, "spk_1")];
        let texts = vec!["hello there".to_string(), "  ".to_string()];
        assert_eq!(split_rows(&pieces, &texts), None);
        let texts = vec![String::new(), "general kenobi".to_string()];
        assert_eq!(split_rows(&pieces, &texts), None);
    }

    #[test]
    fn rows_kept_whole_keep_the_majority_and_are_marked_mixed() {
        let mut out = RowUpdates::default();
        out.label_mixed_row("r1", "spk_0".to_string());
        assert_eq!(out.row_labels, vec![("r1".to_string(), Some("spk_0".to_string()))]);
        assert_eq!(out.mixed_rows, vec!["r1".to_string()]);
        assert!(out.row_splits.is_empty());
    }

    #[test]
    fn row_splits_when_every_piece_has_text() {
        let pieces = vec![turn(0.0, 2.0, "spk_0"), turn(2.0, 4.5, "spk_1")];
        let texts = vec![" hello there ".to_string(), "general kenobi".to_string()];
        assert_eq!(
            split_rows(&pieces, &texts),
            Some(vec![split_row("hello there", 0.0, 2.0, "spk_0"), split_row("general kenobi", 2.0, 4.5, "spk_1")])
        );
    }

    #[test]
    fn row_is_not_split_when_texts_do_not_match_its_pieces() {
        let pieces = vec![turn(0.0, 2.0, "spk_0"), turn(2.0, 4.5, "spk_1")];
        let texts = vec!["hello there".to_string()];
        assert_eq!(split_rows(&pieces, &texts), None);
    }

    #[test]
    fn pieces_map_to_their_sample_ranges() {
        let pieces = vec![turn(10.0, 12.5, "spk_0"), turn(12.5, 15.0, "spk_2")];
        let ranges = piece_ranges(&pieces, TimeMap::Identity, 16_000 * 20).expect("every piece has audio");
        assert_eq!(ranges, vec![160_000..200_000, 200_000..240_000]);
    }

    #[test]
    fn only_a_fast_engine_retranscribes_pieces() {
        use crate::whisper_engine::WhisperCompiledBackend as B;
        assert!(engine_is_fast(Some("parakeet"), B::Cpu));
        assert!(!engine_is_fast(Some("localWhisper"), B::Cpu));
        assert!(!engine_is_fast(None, B::Cpu));
        assert!(engine_is_fast(Some("localWhisper"), B::Cuda));
        assert!(engine_is_fast(None, B::Metal));
    }

    #[test]
    fn row_text_is_split_at_the_speaker_change() {
        let span = RowSpan { start_s: 10.0, end_s: 20.0 };
        let pieces = vec![turn(10.0, 15.0, "spk_0"), turn(15.0, 20.0, "spk_1")];
        let text = "Hello there, how are you doing today? I am fine thanks for asking.";
        assert_eq!(
            split_row_text(text, span, &pieces),
            Some(vec![
                split_row("Hello there, how are you doing today?", 10.0, 15.0, "spk_0"),
                split_row("I am fine thanks for asking.", 15.0, 20.0, "spk_1"),
            ])
        );
    }

    #[test]
    fn uncuttable_row_text_is_not_split() {
        let span = RowSpan { start_s: 0.0, end_s: 10.0 };
        let pieces = vec![turn(0.0, 5.0, "spk_0"), turn(5.0, 10.0, "spk_1")];
        assert_eq!(split_row_text("Yes.", span, &pieces), None);
    }

    #[test]
    fn row_is_not_split_when_a_piece_has_no_audio() {
        let past_the_end = vec![turn(0.0, 2.0, "spk_0"), turn(2.0, 4.0, "spk_1")];
        assert!(piece_ranges(&past_the_end, TimeMap::Identity, 16_000 * 2).is_none());
    }

    #[tokio::test]
    async fn transcripts_json_goes_to_the_stored_meeting_folder() {
        use crate::database::test_support::{migrated_pool, seed_meeting};
        let pool = migrated_pool().await;
        seed_meeting(&pool, "stored", &[]).await;
        seed_meeting(&pool, "no-folder", &[]).await;
        sqlx::query("UPDATE meetings SET folder_path = ? WHERE id = ?")
            .bind("/meetings/stored")
            .bind("stored")
            .execute(&pool)
            .await
            .unwrap();
        let given = Path::new("/from/request");
        assert_eq!(transcripts_json_folder(&pool, "stored", Some(given)).await, Some(PathBuf::from("/meetings/stored")));
        assert_eq!(transcripts_json_folder(&pool, "stored", None).await, Some(PathBuf::from("/meetings/stored")));
        assert_eq!(transcripts_json_folder(&pool, "no-folder", Some(given)).await.as_deref(), Some(given));
        assert_eq!(transcripts_json_folder(&pool, "missing", Some(given)).await.as_deref(), Some(given));
        assert_eq!(transcripts_json_folder(&pool, "no-folder", None).await, None);
    }

    #[tokio::test]
    async fn automatic_job_waits_until_the_recording_stops() {
        let polls = std::sync::atomic::AtomicUsize::new(0);
        let waits = std::sync::atomic::AtomicUsize::new(0);
        let recording = || {
            let n = polls.fetch_add(1, Ordering::SeqCst);
            async move { n < 3 }
        };
        wait_while_recording(recording, &|| false, || { waits.fetch_add(1, Ordering::SeqCst); }, Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 4, "polled until the recording stopped");
        assert_eq!(waits.load(Ordering::SeqCst), 1, "the waiting message is sent once");
    }

    #[tokio::test]
    async fn job_is_not_held_without_a_recording() {
        let waits = std::sync::atomic::AtomicUsize::new(0);
        wait_while_recording(|| async { false }, &|| false, || { waits.fetch_add(1, Ordering::SeqCst); }, Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(waits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn waiting_for_the_recording_stops_when_cancelled() {
        let err = wait_while_recording(|| async { true }, &|| true, || {}, Duration::from_millis(1)).await.unwrap_err();
        assert!(err.is::<Cancelled>());
    }

    #[tokio::test]
    async fn wait_for_audio_finds_a_file_that_appears_late() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.mp4");
        let writer = {
            let path = path.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                std::fs::write(path, b"audio").unwrap();
            })
        };
        let found = wait_for_audio(dir.path(), Duration::from_secs(5), &|| false).await.unwrap();
        assert_eq!(found, path);
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn wait_for_audio_gives_up_when_no_file_appears() {
        let dir = tempfile::tempdir().unwrap();
        assert!(wait_for_audio(dir.path(), Duration::from_millis(200), &|| false).await.is_err());
        assert!(wait_for_audio(dir.path(), Duration::ZERO, &|| false).await.is_err());
    }

    #[tokio::test]
    async fn wait_for_audio_waits_until_the_file_stops_growing() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.mp4");
        std::fs::write(&path, b"a").unwrap();
        let writer = {
            let path = path.clone();
            tokio::spawn(async move {
                for _ in 0..15 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
                    f.write_all(b"a").unwrap();
                }
            })
        };
        let found = wait_for_audio(dir.path(), Duration::from_secs(5), &|| false).await.unwrap();
        assert_eq!(found, path);
        // Returned only after the writer stopped: the file holds all 16 bytes.
        assert_eq!(std::fs::metadata(&found).unwrap().len(), 16);
        assert!(writer.is_finished());
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn wait_for_audio_stops_when_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let err = wait_for_audio(dir.path(), Duration::from_secs(5), &|| true).await.unwrap_err();
        assert!(err.is::<Cancelled>());
    }

    use crate::database::repositories::person::PeopleRepository;
    use crate::database::repositories::speaker::{NameSource, SpeakerLink, SuggestionSource};
    use crate::database::test_support::{migrated_pool, seed_meeting, seed_person, seed_speakers, speakers_by_key, write_speakers};
    use crate::diarization::diarizer::SpeakerCentroid;

    fn centroids(voices: &[(&str, &[f32])]) -> Diarization {
        Diarization {
            turns: vec![],
            overlap: vec![],
            speakers: voices
                .iter()
                .map(|(key, e)| SpeakerCentroid { key: key.to_string(), embedding: e.to_vec(), speech_seconds: 1.0 })
                .collect(),
        }
    }

    fn stored(key: &str, embedding: &[f32], name: Option<&str>, link: SpeakerLink) -> NewSpeaker {
        NewSpeaker { key: key.into(), display_name: name.map(str::to_string), embedding: embedding.to_vec(), speech_seconds: 1.0, link }
    }

    fn linked(person_id: &str, source: NameSource) -> SpeakerLink {
        SpeakerLink { person_id: Some(person_id.into()), name_source: Some(source), ..Default::default() }
    }

    /// The speaker write of an Identify run with voices remembered, as `run_identify` does it.
    async fn rerun(pool: &SqlitePool, meeting_id: &str, d: &Diarization) {
        rerun_with(pool, meeting_id, d, true).await;
    }

    async fn rerun_with(pool: &SqlitePool, meeting_id: &str, d: &Diarization, remember_voices: bool) {
        let mut conn = pool.acquire().await.unwrap();
        let mut tx = sqlx::Connection::begin(&mut *conn).await.unwrap();
        let speakers = speaker_write_names(&mut tx, meeting_id, d, remember_voices).await.unwrap();
        SpeakersRepository::replace_for_meeting(&mut tx, meeting_id, &SpeakerWrite { speakers, ..Default::default() })
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn rerun_keeps_person_link_source_and_suggestion() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_person(&pool, "person-sam", "Sam").await;
        seed_person(&pool, "person-ana", "Ana").await;
        let suggestion = SpeakerLink {
            suggested_person_id: Some("person-ana".into()),
            suggested_name: Some("Ana".into()),
            suggestion_source: Some(SuggestionSource::Voice),
            suggestion_reason: Some("voice match 0.68".into()),
            ..Default::default()
        };
        seed_speakers(
            &pool,
            "m1",
            vec![
                stored("spk_0", &[1.0, 0.0, 0.0], Some("Noah"), linked("person-noah", NameSource::Voice)),
                stored("spk_1", &[0.0, 1.0, 0.0], Some("Sam"), linked("person-sam", NameSource::User)),
                stored("spk_2", &[0.0, 0.0, 1.0], None, suggestion.clone()),
            ],
        )
        .await;

        // The new run numbers the same voices differently.
        rerun(&pool, "m1", &centroids(&[("spk_0", &[0.0, 0.99, 0.1]), ("spk_1", &[0.99, 0.1, 0.0]), ("spk_2", &[0.1, 0.0, 0.99])])).await;

        let s = speakers_by_key(&pool, "m1").await;
        assert_eq!(s["spk_1"].display_name.as_deref(), Some("Noah"));
        assert_eq!(s["spk_1"].link, linked("person-noah", NameSource::Voice));
        assert_eq!(s["spk_0"].display_name.as_deref(), Some("Sam"));
        assert_eq!(s["spk_0"].link, linked("person-sam", NameSource::User));
        assert_eq!(s["spk_2"].display_name, None);
        assert_eq!(s["spk_2"].link, suggestion);
    }

    #[tokio::test]
    async fn rejections_survive_a_rerun() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_speakers(
            &pool,
            "m1",
            vec![stored("spk_0", &[1.0, 0.0], None, SpeakerLink::default()), stored("spk_1", &[0.0, 1.0], None, SpeakerLink::default())],
        )
        .await;
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::add_rejection_conn(&mut conn, "m1", "spk_0", "person-noah").await.unwrap();
        drop(conn);

        // The rejected voice comes back as spk_1.
        rerun(&pool, "m1", &centroids(&[("spk_0", &[0.0, 1.0]), ("spk_1", &[1.0, 0.0])])).await;

        let mut conn = pool.acquire().await.unwrap();
        let rejected = PeopleRepository::rejections_conn(&mut conn, "m1").await.unwrap();
        assert_eq!(rejected, HashSet::from([("spk_1".to_string(), "person-noah".to_string())]));
    }

    use crate::database::repositories::speaker::ReassignTarget;
    use crate::database::test_support::SeedRow;
    use crate::diarization::people::exemplars_conn;

    /// Meeting "a": Noah [1, 0, 0] and Ana [0, 0, 1], both named by the user.
    async fn noah_and_ana_named_in_a(pool: &SqlitePool) {
        seed_person(pool, "person-noah", "Noah").await;
        seed_person(pool, "person-ana", "Ana").await;
        seed_speakers(
            pool,
            "a",
            vec![
                stored("spk_0", &[1.0, 0.0, 0.0], Some("Noah"), linked("person-noah", NameSource::User)),
                stored("spk_1", &[0.0, 0.0, 1.0], Some("Ana"), linked("person-ana", NameSource::User)),
            ],
        )
        .await;
    }

    /// Strong Noah (0.99), weak Ana (0.70) and nobody.
    fn strong_weak_and_unknown() -> Diarization {
        centroids(&[("spk_0", &[0.99, 0.14, 0.0]), ("spk_1", &[0.0, 0.7, 0.68]), ("spk_2", &[0.0, 1.0, 0.0])])
    }

    #[tokio::test]
    async fn identify_links_strong_voice_and_suggests_weak() {
        let pool = migrated_pool().await;
        noah_and_ana_named_in_a(&pool).await;
        seed_meeting(&pool, "b", &[]).await;

        rerun(&pool, "b", &strong_weak_and_unknown()).await;

        let s = speakers_by_key(&pool, "b").await;
        assert_eq!(s["spk_0"].display_name.as_deref(), Some("Noah"));
        assert_eq!(s["spk_0"].link, linked("person-noah", NameSource::Voice));
        assert_eq!(s["spk_1"].display_name, None);
        assert_eq!(
            s["spk_1"].link,
            SpeakerLink {
                suggested_person_id: Some("person-ana".into()),
                suggested_name: Some("Ana".into()),
                suggestion_source: Some(SuggestionSource::Voice),
                suggestion_reason: Some("voice match 0.70".into()),
                ..Default::default()
            }
        );
        assert_eq!(s["spk_2"].display_name, None);
        assert_eq!(s["spk_2"].link, SpeakerLink::default());
    }

    #[tokio::test]
    async fn voice_matching_off_changes_nothing() {
        let pool = migrated_pool().await;
        noah_and_ana_named_in_a(&pool).await;
        seed_meeting(&pool, "b", &[]).await;

        rerun_with(&pool, "b", &strong_weak_and_unknown(), false).await;

        for s in speakers_by_key(&pool, "b").await.values() {
            assert_eq!(s.display_name, None, "{}", s.speaker_key);
            assert_eq!(s.link, SpeakerLink::default(), "{}", s.speaker_key);
        }
    }

    #[tokio::test]
    async fn auto_links_are_not_exemplars() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_speakers(&pool, "a", vec![stored("spk_0", &[1.0, 0.0], Some("Noah"), linked("person-noah", NameSource::Voice))]).await;
        seed_speakers(&pool, "c", vec![stored("spk_0", &[0.0, 1.0], Some("Noah"), linked("person-noah", NameSource::Conversation))]).await;
        seed_meeting(&pool, "b", &[]).await;

        rerun(&pool, "b", &centroids(&[("spk_0", &[1.0, 0.0]), ("spk_1", &[0.0, 1.0])])).await;

        let s = speakers_by_key(&pool, "b").await;
        assert_eq!(s["spk_0"].link, SpeakerLink::default());
        assert_eq!(s["spk_1"].link, SpeakerLink::default());
        let mut conn = pool.acquire().await.unwrap();
        assert!(exemplars_conn(&mut conn, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn legacy_named_speaker_keeps_its_name_on_rerun() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_speakers(&pool, "a", vec![stored("spk_0", &[1.0, 0.0], Some("Noah"), linked("person-noah", NameSource::User))]).await;
        // Named before people existed: a name, no person, no source.
        seed_speakers(&pool, "b", vec![stored("spk_0", &[1.0, 0.0], Some("Bob"), SpeakerLink::default())]).await;

        rerun(&pool, "b", &centroids(&[("spk_0", &[1.0, 0.0])])).await;

        let s = speakers_by_key(&pool, "b").await;
        assert_eq!(s["spk_0"].display_name.as_deref(), Some("Bob"));
        assert_eq!(s["spk_0"].link, SpeakerLink::default());
        let mut conn = pool.acquire().await.unwrap();
        let exemplars = exemplars_conn(&mut conn, None).await.unwrap();
        assert_eq!(exemplars.len(), 1);
        assert_eq!(exemplars[0].exemplars.len(), 1, "the legacy name teaches no voice");
    }

    #[tokio::test]
    async fn handmade_speaker_without_voice_is_left_alone() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "a", &[SeedRow { id: "a1", start: Some(0.0), end: Some(1.0), speaker: None, text: "hi" }]).await;
        let key = SpeakersRepository::reassign_row(&pool, "a", "a1", ReassignTarget::New).await.unwrap();
        SpeakersRepository::name(&pool, "a", &key, "Noah", true).await.unwrap();
        seed_meeting(&pool, "b", &[]).await;

        rerun(&pool, "b", &centroids(&[("spk_0", &[1.0, 0.0])])).await;

        assert_eq!(speakers_by_key(&pool, "b").await["spk_0"].link, SpeakerLink::default());
        assert_eq!(speakers_by_key(&pool, "a").await[&key].display_name.as_deref(), Some("Noah"));
        let mut conn = pool.acquire().await.unwrap();
        assert!(exemplars_conn(&mut conn, None).await.unwrap().is_empty(), "a speaker without a voice teaches nothing");
    }

    #[tokio::test]
    async fn rejected_person_is_not_matched_after_rerun() {
        let pool = migrated_pool().await;
        seed_person(&pool, "person-noah", "Noah").await;
        seed_speakers(&pool, "a", vec![stored("spk_0", &[1.0, 0.0], Some("Noah"), linked("person-noah", NameSource::User))]).await;
        seed_meeting(&pool, "b", &[]).await;
        rerun(&pool, "b", &centroids(&[("spk_0", &[1.0, 0.0]), ("spk_1", &[0.0, 1.0])])).await;
        assert_eq!(speakers_by_key(&pool, "b").await["spk_0"].link, linked("person-noah", NameSource::Voice));

        SpeakersRepository::reject(&pool, "b", "spk_0").await.unwrap();
        // Re-run with the voices numbered the other way round.
        rerun(&pool, "b", &centroids(&[("spk_0", &[0.0, 1.0]), ("spk_1", &[1.0, 0.0])])).await;

        let s = speakers_by_key(&pool, "b").await;
        assert_eq!(s["spk_1"].display_name, None);
        assert_eq!(s["spk_1"].link, SpeakerLink::default(), "neither linked nor suggested again");
        assert_eq!(s["spk_0"].link, SpeakerLink::default());
    }

    #[test]
    fn is_busy_follows_ensure_idle() {
        let id = "busy-test-meeting";
        assert!(!is_busy(id));
        let claim = claim_for_retranscription(id).unwrap();
        assert!(is_busy(id));
        drop(claim);
        assert!(!is_busy(id));
    }

    use crate::diarization::naming::test_support::{answer, FakeNamingModel};

    use crate::database::repositories::setting::SettingsRepository;

    #[tokio::test]
    async fn an_automatic_request_is_declined_unless_the_saved_model_is_local_or_cloud_is_allowed() {
        let pool = migrated_pool().await;
        SettingsRepository::save_model_config(&pool, "claude", "sonnet", "large-v3", None).await.unwrap();
        assert!(naming_request(&pool, "m".into(), true, None).await.is_none(), "nothing is queued");
        assert!(naming_request(&pool, "m".into(), true, Some(false)).await.is_none());
        let allowed = naming_request(&pool, "m".into(), true, Some(true)).await.expect("queued");
        assert_eq!(allowed.kind, JobKind::Naming { allow_cloud: true });
        let manual = naming_request(&pool, "m".into(), false, None).await.expect("a manual request always queues");
        assert!(!manual.automatic);

        SettingsRepository::save_model_config(&pool, "ollama", "llama3", "large-v3", Some("http://localhost:11434")).await.unwrap();
        let local = naming_request(&pool, "m".into(), true, None).await.expect("a local model is used automatically");
        assert_eq!(local.kind, JobKind::Naming { allow_cloud: false });
        SettingsRepository::save_model_config(&pool, "ollama", "llama3", "large-v3", Some("http://10.0.0.5:11434")).await.unwrap();
        assert!(naming_request(&pool, "m".into(), true, None).await.is_none(), "Ollama on another machine is not local");
    }

    #[test]
    fn an_automatic_job_without_consent_stops_when_the_model_is_no_longer_local() {
        assert!(ensure_may_send(true, false, true).is_ok());
        assert!(ensure_may_send(true, true, false).is_ok());
        assert!(ensure_may_send(false, false, false).is_ok(), "jobs the user started always run");
        assert!(ensure_may_send(true, false, false).is_err());
    }

    #[test]
    fn a_skipped_automatic_job_reports_an_automatic_error() {
        let naming = IdentifyRequest { automatic: true, ..naming("m") };
        let payload = error_payload(&naming, "The summary model changed", false);
        assert_eq!(payload["automatic"], true);
        assert_eq!(payload["kind"], "naming");
    }

    #[test]
    fn naming_and_identify_share_the_queue() {
        let mut q = JobQueue::default();
        q.push(req("a")).unwrap();
        assert_eq!(
            q.push(naming("a")),
            Err("Speaker identification is already queued or running for this meeting".to_string()),
            "a meeting with a queued identify job refuses naming"
        );
        q.push(naming("b")).unwrap();
        assert_eq!(
            q.push(req("b")),
            Err("Finding names is already queued or running for this meeting".to_string()),
            "a meeting with a queued naming job refuses identify and says why"
        );
        assert_eq!(ensure_idle_locked(&q, "b"), Err(NAMING_MESSAGE.to_string()), "speaker edits wait for the naming job");
        assert_eq!(ensure_idle_locked(&q, "a"), Err(IDENTIFYING_MESSAGE.to_string()));
        // The queued status tells the banner which job waits.
        assert_eq!(serde_json::to_value(q.status("b").unwrap()).unwrap()["kind"], "naming");
        assert_eq!(q.start_next().unwrap().kind.name(), "identify");
        assert_eq!(q.start_next().unwrap().kind.name(), "naming");
    }

    #[test]
    fn job_events_carry_kind_and_counts() {
        let naming = IdentifyRequest { automatic: true, ..naming("m") };
        assert_eq!(
            complete_payload(&naming, &JobOutcome::Naming(NamingOutcome { named: 2, suggested: 1 })),
            serde_json::json!({
                "meeting_id": "m", "kind": "naming", "speaker_count": 0, "automatic": true,
                "warning": null, "named": 2, "suggested": 1
            })
        );
        let identify = complete_payload(&req("m"), &JobOutcome::Identify(IdentifyOutcome { speaker_count: 3, warning: None }));
        assert_eq!(identify["kind"], "identify");
        assert_eq!(identify["speaker_count"], 3);
        assert_eq!(identify["named"], 0);
        assert_eq!(
            error_payload(&naming, "No summary model is configured", false),
            serde_json::json!({
                "meeting_id": "m", "kind": "naming", "error": "No summary model is configured",
                "automatic": true, "cancelled": false
            })
        );
    }

    /// Noah introduces himself, Ana is asked and answers, Bea is asked but never answers.
    fn conversation() -> Vec<SeedRow> {
        vec![
            SeedRow { id: "r0", start: Some(5.0), end: Some(11.0), speaker: Some("spk_0"), text: "Hi everyone, I'm Noah and I run the platform team." },
            SeedRow { id: "r1", start: Some(12.0), end: Some(19.0), speaker: Some("spk_1"), text: "Thanks Noah. Ana, can you start with the numbers?" },
            SeedRow { id: "r2", start: Some(20.0), end: Some(30.0), speaker: Some("spk_2"), text: "Sure, the annual numbers look good." },
            SeedRow { id: "r3", start: Some(31.0), end: Some(34.0), speaker: Some("spk_1"), text: "Great. Bea, are you there?" },
            SeedRow { id: "r4", start: Some(35.0), end: Some(40.0), speaker: Some("spk_0"), text: "I think Bea had to step out." },
            SeedRow { id: "r5", start: Some(41.0), end: Some(49.0), speaker: Some("spk_2"), text: "Has anyone heard from José?" },
            SeedRow { id: "r6", start: Some(50.0), end: Some(52.0), speaker: Some("spk_3"), text: "Sorry, I was on mute." },
            SeedRow { id: "r7", start: Some(53.0), end: Some(55.0), speaker: None, text: "unlabelled row" },
        ]
    }

    fn unnamed(key: &str) -> NewSpeaker {
        NewSpeaker { key: key.into(), embedding: vec![1.0, 0.0], speech_seconds: 1.0, ..Default::default() }
    }

    fn four_unnamed() -> Vec<NewSpeaker> {
        ["spk_0", "spk_1", "spk_2", "spk_3"].into_iter().map(unnamed).collect()
    }

    /// Meeting "m" with the conversation rows, `people` as (id, name) and `speakers`.
    async fn naming_meeting(people: &[(&str, &str)], speakers: Vec<NewSpeaker>) -> SqlitePool {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "m", &conversation()).await;
        for (id, name) in people {
            seed_person(&pool, id, name).await;
        }
        write_speakers(&pool, "m", speakers).await;
        pool
    }

    async fn seed_summary(pool: &SqlitePool, meeting_id: &str, markdown: &str) {
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO transcript_chunks (meeting_id, transcript_text, model, model_name, created_at)
             VALUES (?, '', 'ollama', 'gemma3:1b', ?)",
        )
        .bind(meeting_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result) VALUES (?, 'completed', ?, ?, ?)")
            .bind(meeting_id)
            .bind(now)
            .bind(now)
            .bind(serde_json::json!({ "markdown": markdown }).to_string())
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn name_from_conversation_applies_and_suggests() {
        let pool = naming_meeting(&[], four_unnamed()).await;
        seed_summary(&pool, "m", "Attendees: Noah, Ana, Bea").await;
        let model = FakeNamingModel::new(
            8000,
            vec![Ok(answer(&[
                ("spk_0", "Noah", "I'm Noah", "self_intro", "high"),
                ("spk_3", "Bea", "Bea, are you there?", "addressed", "high"),
            ]))],
        );
        let outcome = name_from_conversation(&pool, "m", &model, std::future::pending()).await.unwrap();
        assert_eq!(outcome, NamingOutcome { named: 1, suggested: 1 });

        let speakers = SpeakersRepository::list(&pool, "m").await.unwrap();
        assert_eq!(speakers[0].display_name.as_deref(), Some("Noah"));
        assert_eq!(speakers[0].link.name_source, Some(NameSource::Conversation));
        assert_eq!(speakers[0].link.person_id, None);
        assert_eq!(speakers[3].display_name, None);
        assert_eq!(speakers[3].link.suggested_name.as_deref(), Some("Bea"));
        assert_eq!(speakers[3].link.suggestion_source, Some(SuggestionSource::Conversation));
        assert_eq!(speakers[3].link.suggestion_reason.as_deref(), Some("addressed as Bea at 00:31"));

        let (_, user) = model.prompts().remove(0);
        assert!(user.contains("[00:05] spk_0: Hi everyone, I'm Noah and I run the platform team."));
        assert!(user.contains("Attendees: Noah, Ana, Bea"));
        assert!(!user.contains("unlabelled row"));
    }

    #[tokio::test]
    async fn name_from_conversation_without_valid_names_changes_nothing() {
        let pool = naming_meeting(&[], four_unnamed()).await;
        let before = SpeakersRepository::list(&pool, "m").await.unwrap();

        // A quote that is not in the transcript.
        let model = FakeNamingModel::new(8000, vec![Ok(answer(&[("spk_0", "Noah", "Noah left early", "self_intro", "high")]))]);
        assert_eq!(name_from_conversation(&pool, "m", &model, std::future::pending()).await.unwrap(), NamingOutcome::default());
        // An answer that cannot be read.
        let model = FakeNamingModel::new(8000, vec![Ok("I don't know who these people are.".into())]);
        let err = name_from_conversation(&pool, "m", &model, std::future::pending()).await.unwrap_err();
        assert_eq!(format!("{err:#}"), "The model's answer could not be read");
        assert_eq!(SpeakersRepository::list(&pool, "m").await.unwrap(), before);

        // A meeting without speakers is refused before the model is asked.
        seed_meeting(&pool, "no-speakers", &[]).await;
        let model = FakeNamingModel::new(8000, vec![]);
        let err = name_from_conversation(&pool, "no-speakers", &model, std::future::pending()).await.unwrap_err();
        assert_eq!(format!("{err:#}"), "Identify speakers first");
        assert!(model.prompts().is_empty());
    }

    #[tokio::test]
    async fn name_from_conversation_never_touches_named_or_voice_speakers() {
        let typed = NewSpeaker {
            display_name: Some("Ana".into()),
            link: SpeakerLink { person_id: Some("person-ana".into()), name_source: Some(NameSource::User), ..Default::default() },
            ..unnamed("spk_0")
        };
        let voice_linked = NewSpeaker {
            display_name: Some("Noah".into()),
            link: SpeakerLink { person_id: Some("person-noah".into()), name_source: Some(NameSource::Voice), ..Default::default() },
            ..unnamed("spk_1")
        };
        let voice_suggested = NewSpeaker {
            link: SpeakerLink {
                suggested_person_id: Some("person-bea".into()),
                suggested_name: Some("Bea".into()),
                suggestion_source: Some(SuggestionSource::Voice),
                suggestion_reason: Some("voice match 0.66".into()),
                ..Default::default()
            },
            ..unnamed("spk_2")
        };
        let pool = naming_meeting(
            &[("person-ana", "Ana"), ("person-noah", "Noah"), ("person-bea", "Bea"), ("person-zoe", "Zoe")],
            vec![typed, voice_linked, voice_suggested, unnamed("spk_3")],
        )
        .await;
        // Zoe exists only to anchor a rejection ("spk_3 is not Zoe").
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::add_rejection_conn(&mut conn, "m", "spk_3", "person-zoe").await.unwrap();
        drop(conn);
        let before = SpeakersRepository::list(&pool, "m").await.unwrap();
        let model = FakeNamingModel::new(
            8000,
            vec![Ok(answer(&[
                ("spk_0", "Noah", "I'm Noah", "self_intro", "high"),
                ("spk_1", "Ana", "Ana, can you start", "addressed", "high"),
                ("spk_2", "José", "heard from José", "mentioned", "high"),
            ]))],
        );
        assert_eq!(name_from_conversation(&pool, "m", &model, std::future::pending()).await.unwrap(), NamingOutcome::default());
        assert_eq!(SpeakersRepository::list(&pool, "m").await.unwrap(), before);
        // Only people linked to a speaker are offered to the model: Bea is only a suggestion and
        // Zoe only a rejection.
        let (_, user) = model.prompts().remove(0);
        assert!(user.contains("People named in earlier meetings: Ana, Noah\n"), "{user}");
        assert!(!user.contains("Zoe"));
    }

    #[tokio::test]
    async fn cancelled_naming_writes_nothing() {
        let pool = naming_meeting(&[], four_unnamed()).await;
        let before = SpeakersRepository::list(&pool, "m").await.unwrap();
        let model = FakeNamingModel::new(8000, vec![Ok(answer(&[("spk_0", "Noah", "I'm Noah", "self_intro", "high")]))]);

        let err = name_from_conversation(&pool, "m", &model, std::future::ready(())).await.unwrap_err();

        assert!(err.is::<Cancelled>(), "{err:#}");
        assert_eq!(SpeakersRepository::list(&pool, "m").await.unwrap(), before);
    }

    #[tokio::test]
    async fn name_from_conversation_skips_names_rejected_for_the_speaker() {
        let pool = naming_meeting(&[("person-noah", "Noah")], four_unnamed()).await;
        let mut conn = pool.acquire().await.unwrap();
        PeopleRepository::add_rejection_conn(&mut conn, "m", "spk_0", "person-noah").await.unwrap();
        drop(conn);
        let reply = answer(&[("spk_0", "noah", "I'm Noah", "self_intro", "high")]);
        let model = FakeNamingModel::new(8000, vec![Ok(reply)]);
        assert_eq!(name_from_conversation(&pool, "m", &model, std::future::pending()).await.unwrap(), NamingOutcome::default());
        assert_eq!(SpeakersRepository::list(&pool, "m").await.unwrap()[0].display_name, None);
    }

    struct HangingModel;

    #[async_trait::async_trait]
    impl NamingModel for HangingModel {
        async fn complete(&self, _system: &str, _user: &str) -> Result<String, String> {
            std::future::pending().await
        }

        fn context_tokens(&self) -> usize {
            8000
        }
    }

    #[tokio::test]
    async fn cancelling_stops_a_model_call_that_never_answers() {
        let pool = naming_meeting(&[], four_unnamed()).await;
        let before = SpeakersRepository::list(&pool, "m").await.unwrap();
        let cancel_soon = tokio::time::sleep(Duration::from_millis(20));
        let result = tokio::time::timeout(Duration::from_secs(5), name_from_conversation(&pool, "m", &HangingModel, cancel_soon))
            .await
            .expect("cancel ends the wait");
        assert!(result.unwrap_err().is::<Cancelled>());
        assert_eq!(SpeakersRepository::list(&pool, "m").await.unwrap(), before);
    }
}
