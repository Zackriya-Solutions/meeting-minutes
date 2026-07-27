//! Deep Analytics multi-stage pipeline (background task).
//!
//! Orchestrates: interactive speaker confirmation (LLM name guesses + merge proposals,
//! applied to the speakers DB on user confirm) -> deterministic dynamics -> 8 DeepSeek
//! extraction stages -> synthesis -> local score + HTML render. Progress is streamed to
//! the frontend via Tauri events and mirrored into the `analytics_reports` row. The whole
//! report fails ONLY on: no transcript, privacy block, missing DeepSeek credentials, or
//! cancellation. Every other per-stage failure is soft — the stage is recorded as failed
//! and its section renders a "не удалось построить" placeholder while the pipeline
//! continues.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::database::repositories::analytics_report::{AnalyticsReportsRepository, TOTAL_STAGES};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::speaker::{
    MeetingSpeaker, SpeakersRepository, TranscriptSpeakerSegment,
};
use crate::llm::providers::{deepseek, resolve_deepseek};
use crate::llm::{ensure_outbound_allowed, Purpose};
use crate::report::dynamics::{self, DynSegment, Dynamics};
use crate::report::prompts::{
    self, Clarify, ClarifyAnswer, ClarifyQuestion, Classification, Commitments,
    DisagreementsConcepts, Insights, Numbers, Roles, SpeakerDecision, SpeakerGuesses,
    SpeakerLine, SpeakerSuggestion, ThreadsRisks, Topics,
};
use crate::report::render::{compute_score, render_report, RenderInput};

/// Stage id + short Russian progress label, in pipeline order. `stage_index` emitted to
/// the frontend is 1-based (position + 1); `total_stages` is [`TOTAL_STAGES`].
const STAGE_META: [(&str, &str); TOTAL_STAGES as usize] = [
    ("speakers", "Определение спикеров"),
    ("dynamics", "Анализ динамики разговора"),
    ("classify", "Классификация встречи"),
    ("clarify", "Уточняющие вопросы"),
    ("topics", "Темы и повестка"),
    ("decisions", "Решения"),
    ("commitments", "Обязательства"),
    ("threads_risks", "Незакрытое и риски"),
    ("disagreements_concepts", "Разногласия и концепции"),
    ("numbers", "Числа встречи"),
    ("roles", "Роли на встрече"),
    ("insights", "Главное — синтез"),
    ("render", "Сборка отчёта"),
];

/// How long the pipeline waits at an interactive pause (speaker confirmation, clarify
/// answers) before it gives up and proceeds with no input.
const INPUT_WAIT: Duration = Duration::from_secs(600);

/// Sampling temperature for all structured extraction calls.
const STAGE_TEMPERATURE: f32 = 0.2;

// Cancellation tokens keyed by report id (mirrors the summary pipeline pattern).
static CANCELLATION_REGISTRY: Lazy<Arc<Mutex<HashMap<String, CancellationToken>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

// One-shot channels delivering user answers to a report parked in `waiting_input`.
static ANSWER_REGISTRY: Lazy<Arc<Mutex<HashMap<String, oneshot::Sender<Vec<ClarifyAnswer>>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

// One-shot channels delivering speaker decisions to a report parked in `waiting_input`
// at the speakers stage.
static SPEAKER_REGISTRY: Lazy<Arc<Mutex<HashMap<String, oneshot::Sender<Vec<SpeakerDecision>>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

fn register_token(report_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    if let Ok(mut reg) = CANCELLATION_REGISTRY.lock() {
        reg.insert(report_id.to_string(), token.clone());
    }
    token
}

fn cleanup_token(report_id: &str) {
    if let Ok(mut reg) = CANCELLATION_REGISTRY.lock() {
        reg.remove(report_id);
    }
    cleanup_answer_sender(report_id);
    cleanup_speaker_sender(report_id);
}

fn cleanup_answer_sender(report_id: &str) {
    if let Ok(mut reg) = ANSWER_REGISTRY.lock() {
        reg.remove(report_id);
    }
}

fn cleanup_speaker_sender(report_id: &str) {
    if let Ok(mut reg) = SPEAKER_REGISTRY.lock() {
        reg.remove(report_id);
    }
}

/// Deliver answers to a report currently waiting for input. Idempotent: if nothing is
/// waiting for `report_id`, this is a no-op.
pub fn submit_answers(report_id: &str, answers: Vec<ClarifyAnswer>) {
    let sender = ANSWER_REGISTRY
        .lock()
        .ok()
        .and_then(|mut reg| reg.remove(report_id));
    if let Some(tx) = sender {
        let _ = tx.send(answers);
    }
}

/// Register a waiter and block until answers arrive, the run is cancelled, or the wait
/// times out. Returns `None` only on cancellation (caller then finishes as cancelled);
/// `Some(answers)` on submission (or an empty vec on timeout / dropped sender).
async fn wait_for_answers(report_id: &str, token: &CancellationToken) -> Option<Vec<ClarifyAnswer>> {
    let (tx, rx) = oneshot::channel::<Vec<ClarifyAnswer>>();
    if let Ok(mut reg) = ANSWER_REGISTRY.lock() {
        reg.insert(report_id.to_string(), tx);
    }
    let outcome = tokio::select! {
        received = rx => Some(received.unwrap_or_default()),
        _ = token.cancelled() => None,
        _ = tokio::time::sleep(INPUT_WAIT) => Some(Vec::new()),
    };
    cleanup_answer_sender(report_id);
    outcome
}

/// Deliver speaker decisions to a report currently waiting at the speakers stage.
/// Idempotent: if nothing is waiting for `report_id`, this is a no-op.
pub fn submit_speakers(report_id: &str, decisions: Vec<SpeakerDecision>) {
    let sender = SPEAKER_REGISTRY
        .lock()
        .ok()
        .and_then(|mut reg| reg.remove(report_id));
    if let Some(tx) = sender {
        let _ = tx.send(decisions);
    }
}

/// Same contract as [`wait_for_answers`] but for the speakers pause: `None` only on
/// cancellation; an empty vec (timeout / explicit skip) means "change nothing".
async fn wait_for_speakers(
    report_id: &str,
    token: &CancellationToken,
) -> Option<Vec<SpeakerDecision>> {
    let (tx, rx) = oneshot::channel::<Vec<SpeakerDecision>>();
    if let Ok(mut reg) = SPEAKER_REGISTRY.lock() {
        reg.insert(report_id.to_string(), tx);
    }
    let outcome = tokio::select! {
        received = rx => Some(received.unwrap_or_default()),
        _ = token.cancelled() => None,
        _ = tokio::time::sleep(INPUT_WAIT) => Some(Vec::new()),
    };
    cleanup_speaker_sender(report_id);
    outcome
}

/// Signal cancellation for a running report. Returns true if a live token was found.
pub fn cancel_report(report_id: &str) -> bool {
    if let Ok(reg) = CANCELLATION_REGISTRY.lock() {
        if let Some(token) = reg.get(report_id) {
            token.cancel();
            return true;
        }
    }
    false
}

/// The DeepSeek model this run will use: `deepseek.model` setting, else the provider default.
pub async fn resolve_model(pool: &SqlitePool) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = 'deepseek.model'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| deepseek::DEFAULT_MODEL.to_string())
}

/// Reconcile reports orphaned by an app restart. The pipeline and its interactive
/// input waits (clarify / speakers) live only in memory, so any row still
/// `queued`/`running`/`waiting_input` after a restart can never resume: reopening it
/// re-shows the persisted questions, but submitting answers no-ops because no pipeline
/// is listening. Mark those failed so the meeting can be regenerated. Idempotent; call
/// once at startup before serving report commands.
pub async fn recover_interrupted_reports(pool: &SqlitePool) {
    match AnalyticsReportsRepository::fail_interrupted(
        pool,
        "Генерация отчёта была прервана перезапуском приложения. Запустите её заново.",
    )
    .await
    {
        Ok(0) => {}
        Ok(n) => log::info!("[report] reset {n} interrupted report(s) to failed after restart"),
        Err(e) => log::warn!("[report] could not reconcile interrupted reports: {e}"),
    }
}

// ---- speaker identity helpers (contract-defined fallback order) ----

fn speaker_label(seg: &TranscriptSpeakerSegment) -> String {
    if let Some(dn) = seg.display_name.as_deref().filter(|s| !s.trim().is_empty()) {
        return dn.to_string();
    }
    if let Some(sp) = seg.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
        return sp.to_string();
    }
    if let Some(id) = seg.speaker_id {
        return format!("Спикер {id}");
    }
    "Спикер".to_string()
}

fn speaker_key(seg: &TranscriptSpeakerSegment) -> String {
    if let Some(id) = seg.speaker_id {
        return format!("id:{id}");
    }
    if let Some(sp) = seg.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
        return format!("ch:{sp}");
    }
    format!("lbl:{}", speaker_label(seg))
}

/// Per-segment views the pipeline stages consume, rebuilt after the speakers stage may
/// have changed names/attributions.
fn build_segment_views(
    segments: &[TranscriptSpeakerSegment],
) -> (Vec<DynSegment>, Vec<String>, Vec<String>) {
    let dyn_segments: Vec<DynSegment> = segments
        .iter()
        .map(|s| DynSegment {
            start: s.audio_start_time,
            text: s.text.clone(),
            speaker_key: speaker_key(s),
            speaker_label: speaker_label(s),
        })
        .collect();
    let seg_labels: Vec<String> = dyn_segments.iter().map(|d| d.speaker_label.clone()).collect();
    let seg_texts: Vec<String> = dyn_segments.iter().map(|d| d.text.clone()).collect();
    (dyn_segments, seg_labels, seg_texts)
}

// ---- speakers stage: transcript excerpts for the confirmation dialog ----

/// Max characters kept per quoted line (keeps the persisted payload bounded).
const LINE_CHARS: usize = 220;
/// Lines shorter than this are poor recognition samples ("да", "угу") and are only used
/// when a speaker has nothing longer.
const MIN_SAMPLE_CHARS: usize = 15;
/// Representative lines per speaker.
const SAMPLES_PER_SPEAKER: usize = 4;
/// Lines kept in the excerpt around the name evidence (before + line + after).
const EVIDENCE_BEFORE: usize = 2;
const EVIDENCE_AFTER: usize = 2;
/// Cap on the two-speaker comparison excerpt.
const PAIR_LINES: usize = 6;

/// Trim a transcript line to `max_chars` on a char boundary.
fn clip(text: &str, max_chars: usize) -> String {
    let t = text.trim();
    let mut out: String = t.chars().take(max_chars).collect();
    if t.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// Parallel views of the meeting transcript, used to build the excerpts shown in the
/// speaker-confirmation dialog. `segments`, `timed` and `labels` are index-aligned.
struct SpeakerContext<'a> {
    segments: &'a [TranscriptSpeakerSegment],
    timed: &'a [dynamics::TimedSegment],
    labels: &'a [String],
}

impl SpeakerContext<'_> {
    fn line(&self, i: usize, highlight: bool) -> SpeakerLine {
        let start = self.timed.get(i).map(|t| t.start).unwrap_or(0.0);
        SpeakerLine {
            seg: i as i64,
            time: prompts::fmt_mmss(start),
            speaker_id: self.segments[i].speaker_id,
            label: self.labels.get(i).cloned().unwrap_or_default(),
            text: clip(&self.segments[i].text, LINE_CHARS),
            highlight,
        }
    }

    fn indices_of(&self, speaker_id: i64) -> Vec<usize> {
        (0..self.segments.len())
            .filter(|&i| self.segments[i].speaker_id == Some(speaker_id))
            .collect()
    }

    fn char_len(&self, i: usize) -> usize {
        self.segments[i].text.trim().chars().count()
    }

    /// Share of total speech time and mm:ss of the first line, for one speaker.
    fn stats(&self, speaker_id: i64) -> (f32, String) {
        let idx = self.indices_of(speaker_id);
        let dur = |i: usize| {
            self.timed
                .get(i)
                .map(|t| (t.end - t.start).max(0.0))
                .unwrap_or(0.0)
        };
        let total: f64 = (0..self.segments.len()).map(dur).sum();
        let mine: f64 = idx.iter().copied().map(dur).sum();
        let share = if total > 0.0 { (mine / total) as f32 } else { 0.0 };
        let first = idx
            .first()
            .map(|&i| prompts::fmt_mmss(self.timed.get(i).map(|t| t.start).unwrap_or(0.0)))
            .unwrap_or_default();
        (share, first)
    }

    /// Representative lines: the transcript is split into `limit` equal buckets of this
    /// speaker's turns and the longest line of each bucket is taken, so the samples span
    /// the whole meeting instead of clustering at the start. Short filler lines are
    /// skipped unless the speaker said nothing longer.
    fn samples(&self, speaker_id: i64, limit: usize) -> Vec<SpeakerLine> {
        let all = self.indices_of(speaker_id);
        let substantive: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&i| self.char_len(i) >= MIN_SAMPLE_CHARS)
            .collect();
        let pool = if substantive.is_empty() { &all } else { &substantive };
        let n = pool.len();
        if n == 0 || limit == 0 {
            return Vec::new();
        }
        let k = limit.min(n);
        (0..k)
            .filter_map(|b| {
                let lo = b * n / k;
                let hi = (((b + 1) * n / k).max(lo + 1)).min(n);
                pool[lo..hi]
                    .iter()
                    .copied()
                    .max_by_key(|&i| self.char_len(i))
            })
            .map(|i| self.line(i, false))
            .collect()
    }

    /// The dialogue around one line, with that line highlighted.
    fn window(&self, center: usize, before: usize, after: usize) -> Vec<SpeakerLine> {
        if center >= self.segments.len() {
            return Vec::new();
        }
        let lo = center.saturating_sub(before);
        let hi = (center + after).min(self.segments.len() - 1);
        (lo..=hi).map(|i| self.line(i, i == center)).collect()
    }

    /// Find the line the LLM quoted as name evidence. Prefers the `seg` index it
    /// reported, falling back to a text search for the quote (models often get the
    /// index slightly wrong while quoting accurately).
    fn locate_evidence(&self, seg: i64, quote: &str, speaker_id: i64) -> Option<usize> {
        let n = self.segments.len();
        let reported = (seg >= 0 && (seg as usize) < n).then_some(seg as usize);
        let needle = quote.trim().to_lowercase();
        if needle.is_empty() {
            return reported;
        }
        // Trust the reported index only if the quote is really there.
        if let Some(i) = reported {
            if self.segments[i].text.to_lowercase().contains(&needle) {
                return Some(i);
            }
        }
        // Otherwise search: this speaker's own lines first, then anywhere. If the model
        // paraphrased the quote, fall back to whatever index it reported.
        let hit = |ids: Vec<usize>| {
            ids.into_iter()
                .find(|&i| self.segments[i].text.to_lowercase().contains(&needle))
        };
        hit(self.indices_of(speaker_id))
            .or_else(|| hit((0..n).collect()))
            .or(reported)
    }

    /// Excerpt where two speakers talk closest together — the "who is who" comparison.
    /// Returns an empty vec when they never speak near each other.
    fn pair(&self, a: i64, b: i64, max_lines: usize) -> Vec<SpeakerLine> {
        let mut best: Option<(usize, usize, usize)> = None; // (gap, from, to)
        let mut consider = |lo: usize, hi: usize| {
            let gap = hi - lo;
            let better = match best {
                Some((g, ..)) => gap < g,
                None => true,
            };
            if better {
                best = Some((gap, lo, hi));
            }
        };
        let (mut last_a, mut last_b): (Option<usize>, Option<usize>) = (None, None);
        for i in 0..self.segments.len() {
            match self.segments[i].speaker_id {
                Some(id) if id == a => {
                    if let Some(j) = last_b {
                        consider(j, i);
                    }
                    last_a = Some(i);
                }
                Some(id) if id == b => {
                    if let Some(j) = last_a {
                        consider(j, i);
                    }
                    last_b = Some(i);
                }
                _ => {}
            }
        }
        let Some((_, from, to)) = best else {
            return Vec::new();
        };
        let n = self.segments.len();
        let budget = max_lines.max(2);
        // Always include BOTH turns. When they are close enough to fit the budget, show
        // one contiguous excerpt (padded for context); when they are far apart — common
        // for merge candidates, which is exactly why they were flagged — show a small
        // window around each turn so the second speaker is never truncated away.
        let indices: Vec<usize> = if to - from + 1 <= budget {
            let extra = budget - (to - from + 1);
            let lo = from.saturating_sub(extra / 2);
            let hi = (to + (extra - extra / 2)).min(n - 1);
            (lo..=hi).collect()
        } else {
            let per = (budget / 2).max(1);
            let window = |center: usize| -> Vec<usize> {
                let lo = center.saturating_sub((per - 1) / 2);
                (lo..=(lo + per - 1).min(n - 1)).collect()
            };
            let mut idx = window(from);
            idx.extend(window(to));
            idx.sort_unstable();
            idx.dedup();
            idx
        };
        indices.into_iter().map(|i| self.line(i, false)).collect()
    }
}

// ---- speakers stage: sanitize LLM guesses, apply confirmed decisions ----

/// Combine the meeting's speaker roster with the (possibly absent) LLM guesses into the
/// suggestion rows shown in the dialog. Sanitizes the guesses against the roster:
/// unknown ids are dropped, each speaker joins at most one merge group, a merge target
/// never appears as merged itself, self-merges are ignored, confidence is clamped.
fn build_speaker_suggestions(
    roster: &[MeetingSpeaker],
    guesses: Option<&SpeakerGuesses>,
) -> Vec<SpeakerSuggestion> {
    use std::collections::HashSet;

    let known: HashSet<i64> = roster.iter().map(|s| s.id).collect();

    // First valid name guess per speaker id.
    let mut names: HashMap<i64, (String, f32, String, i64)> = HashMap::new();
    // speaker id -> (merge target, reason)
    let mut merged_into: HashMap<i64, (i64, String)> = HashMap::new();
    let mut keeps: HashSet<i64> = HashSet::new();

    if let Some(g) = guesses {
        for n in &g.names {
            let name = n.name.trim();
            if name.is_empty() || !known.contains(&n.speaker_id) {
                continue;
            }
            names.entry(n.speaker_id).or_insert_with(|| {
                (
                    name.to_string(),
                    n.confidence.clamp(0.0, 1.0),
                    n.evidence.trim().to_string(),
                    n.seg,
                )
            });
        }
        for m in &g.merges {
            if !known.contains(&m.keep_id) || merged_into.contains_key(&m.keep_id) {
                continue;
            }
            let mut inserted = false;
            for id in &m.merge_ids {
                if *id == m.keep_id
                    || !known.contains(id)
                    || merged_into.contains_key(id)
                    || keeps.contains(id)
                {
                    continue;
                }
                merged_into.insert(*id, (m.keep_id, m.reason.trim().to_string()));
                inserted = true;
            }
            if inserted {
                keeps.insert(m.keep_id);
            }
        }
    }

    roster
        .iter()
        .map(|s| {
            let guess = names.get(&s.id);
            let merge = merged_into.get(&s.id);
            SpeakerSuggestion {
                speaker_id: s.id,
                current_name: s.display_name.clone(),
                segment_count: s.segment_count,
                is_confirmed: s.is_confirmed,
                suggested_name: guess.map(|(n, ..)| n.clone()),
                confidence: guess.map(|(_, c, ..)| *c).unwrap_or(0.0),
                evidence: guess
                    .map(|(_, _, e, _)| e.clone())
                    .filter(|e| !e.is_empty()),
                merge_into: merge.map(|(t, _)| *t),
                merge_reason: merge.map(|(_, r)| r.clone()).filter(|r| !r.is_empty()),
                // Filled by `enrich_suggestions` once the transcript context is built.
                ..Default::default()
            }
        })
        .collect()
}

/// Fill in the transcript excerpts the confirmation dialog renders: recognition samples,
/// the dialogue around each name guess, and a two-speaker comparison for every proposed
/// merge. Presentation data only — nothing here changes what gets applied.
fn enrich_suggestions(
    suggestions: &mut [SpeakerSuggestion],
    ctx: &SpeakerContext<'_>,
    guesses: Option<&SpeakerGuesses>,
) {
    // First non-empty-name guess per speaker. This MUST match the first-wins selection
    // in `build_speaker_suggestions` (which used `entry().or_insert_with`): a plain
    // `collect()` keeps the LAST guess for a repeated speaker_id, so the located evidence
    // line could come from a different guess than the displayed name/quote.
    let mut evidence_seg: HashMap<i64, i64> = HashMap::new();
    if let Some(g) = guesses {
        for n in &g.names {
            if n.name.trim().is_empty() {
                continue;
            }
            evidence_seg.entry(n.speaker_id).or_insert(n.seg);
        }
    }

    for s in suggestions.iter_mut() {
        let (share, first_seen) = ctx.stats(s.speaker_id);
        s.talk_share = share;
        s.first_seen = first_seen;
        s.samples = ctx.samples(s.speaker_id, SAMPLES_PER_SPEAKER);

        if s.suggested_name.is_some() {
            let seg = evidence_seg.get(&s.speaker_id).copied().unwrap_or(-1);
            let quote = s.evidence.clone().unwrap_or_default();
            if let Some(i) = ctx.locate_evidence(seg, &quote, s.speaker_id) {
                s.evidence_context = ctx.window(i, EVIDENCE_BEFORE, EVIDENCE_AFTER);
            }
        }
        if let Some(target) = s.merge_into {
            s.merge_context = ctx.pair(s.speaker_id, target, PAIR_LINES);
        }
    }
}

/// Apply the user's confirmed decisions: reattribute merged speakers' segments (this
/// meeting only), then rename kept speakers whose final name differs from the current
/// one. Invalid decisions (unknown ids, merge chains, merges into a merged speaker) are
/// dropped with a warning rather than failing the report. Returns true if anything in
/// the speakers DB actually changed.
async fn apply_speaker_decisions(
    pool: &SqlitePool,
    meeting_id: &str,
    roster: &[MeetingSpeaker],
    decisions: &[SpeakerDecision],
) -> bool {
    use std::collections::HashSet;

    let current: HashMap<i64, &str> = roster
        .iter()
        .map(|s| (s.id, s.display_name.as_str()))
        .collect();

    // A merge target is only valid if it is itself kept (no chains).
    let merged_ids: HashSet<i64> = decisions
        .iter()
        .filter(|d| d.merge_into.is_some())
        .map(|d| d.speaker_id)
        .collect();

    let mut seen: HashSet<i64> = HashSet::new();
    let mut groups: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut renames: Vec<(i64, String)> = Vec::new();

    for d in decisions {
        if !current.contains_key(&d.speaker_id) || !seen.insert(d.speaker_id) {
            continue;
        }
        if let Some(target) = d.merge_into {
            if target == d.speaker_id || !current.contains_key(&target) || merged_ids.contains(&target)
            {
                log::warn!(
                    "[report] dropping invalid merge decision {} -> {target} for meeting {meeting_id}",
                    d.speaker_id
                );
                continue;
            }
            groups.entry(target).or_default().push(d.speaker_id);
            continue;
        }
        if let Some(name) = d.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if current.get(&d.speaker_id).copied() != Some(name) {
                renames.push((d.speaker_id, name.to_string()));
            }
        }
    }

    let mut changed = false;
    for (keep, ids) in &groups {
        match SpeakersRepository::merge_meeting_speakers(pool, meeting_id, *keep, ids).await {
            Ok(n) => {
                changed |= n > 0;
                log::info!(
                    "[report] merged speakers {ids:?} into {keep} for meeting {meeting_id} ({n} segment(s))"
                );
            }
            Err(e) => log::warn!("[report] merge {ids:?} -> {keep} failed: {e}"),
        }
    }
    for (id, name) in &renames {
        match SpeakersRepository::rename(pool, *id, name).await {
            Ok(n) => changed |= n > 0,
            Err(e) => log::warn!("[report] rename speaker {id} failed: {e}"),
        }
    }

    if changed {
        // Merged-away profiles may now be unreferenced; collect them (best-effort).
        match SpeakersRepository::delete_orphaned_unconfirmed(pool).await {
            Ok(0) => {}
            Ok(n) => log::info!("[report] GC removed {n} orphaned speaker profile(s) after merge"),
            Err(e) => log::warn!("[report] orphaned-speaker GC failed (non-fatal): {e}"),
        }
    }
    changed
}

// ---- event / db helpers ----

async fn start_stage<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    report_id: &str,
    meeting_id: &str,
    pos: usize,
) {
    let (id, label) = STAGE_META[pos];
    let stage_index = (pos + 1) as i64;
    if let Err(e) = AnalyticsReportsRepository::update_stage(pool, report_id, id, stage_index).await {
        log::warn!("[report] failed to persist stage {id} for {report_id}: {e}");
    }
    let _ = app.emit(
        "analytics-report-progress",
        json!({
            "report_id": report_id,
            "meeting_id": meeting_id,
            "stage": id,
            "stage_index": stage_index,
            "total_stages": TOTAL_STAGES,
            "label": label,
        }),
    );
}

async fn fail<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    report_id: &str,
    meeting_id: &str,
    msg: &str,
) {
    log::warn!("[report] {report_id} failed: {msg}");
    if let Err(e) = AnalyticsReportsRepository::mark_failed(pool, report_id, msg).await {
        log::error!("[report] failed to mark {report_id} failed: {e}");
    }
    let _ = app.emit(
        "analytics-report-error",
        json!({ "report_id": report_id, "meeting_id": meeting_id, "error": msg }),
    );
    cleanup_token(report_id);
}

async fn finish_cancelled(pool: &SqlitePool, report_id: &str) {
    if let Err(e) = AnalyticsReportsRepository::mark_cancelled(pool, report_id).await {
        log::error!("[report] failed to mark {report_id} cancelled: {e}");
    }
    cleanup_token(report_id);
    log::info!("[report] {report_id} cancelled");
}

// ---- structured stage execution ----

fn strip_json_fences(s: &str) -> String {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    // Isolate the outermost JSON object if the model added stray prose.
    match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b >= a => t[a..=b].to_string(),
        _ => t.to_string(),
    }
}

async fn attempt<T: DeserializeOwned>(
    client: &deepseek::DeepSeekClient,
    system: &str,
    user: &str,
) -> Result<T, String> {
    let raw = client.complete_json(system, user, STAGE_TEMPERATURE).await?;
    let cleaned = strip_json_fences(&raw);
    serde_json::from_str::<T>(&cleaned).map_err(|e| format!("JSON parse failed: {e}"))
}

/// Run one LLM stage: call, parse; on failure retry ONCE with a stricter instruction; on
/// second failure return `None` (soft failure — caller records it and continues).
async fn run_stage<T: DeserializeOwned>(
    client: &deepseek::DeepSeekClient,
    system: &str,
    user: &str,
    stage: &str,
    failed: &mut Vec<String>,
) -> Option<T> {
    match attempt::<T>(client, system, user).await {
        Ok(v) => Some(v),
        Err(first) => {
            log::warn!("[report] stage {stage} first attempt failed: {first}; retrying");
            let retry_user = prompts::retry_suffix(user);
            match attempt::<T>(client, system, &retry_user).await {
                Ok(v) => Some(v),
                Err(second) => {
                    log::warn!("[report] stage {stage} failed after retry: {second}");
                    failed.push(stage.to_string());
                    None
                }
            }
        }
    }
}

fn sanitize_component(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Write the report HTML. Preferred location is the meeting's own folder (alongside its
/// audio/transcript); if `folder_path` is unset, unusable, or the write fails, fall back
/// to `{app_data_dir}/reports/{meeting_id}/`.
fn write_html<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    folder_path: Option<&str>,
    html: &str,
) -> Result<String, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("deep_report_{ts}.html");

    // 1) Meeting's own folder.
    if let Some(fp) = folder_path.map(str::trim).filter(|s| !s.is_empty()) {
        let dir = PathBuf::from(fp);
        let usable = dir.is_dir() || std::fs::create_dir_all(&dir).is_ok();
        if usable {
            let path = dir.join(&filename);
            match std::fs::write(&path, html) {
                Ok(_) => return Ok(path.to_string_lossy().to_string()),
                Err(e) => log::warn!(
                    "[report] could not write into meeting folder {fp}: {e}; using app-data fallback"
                ),
            }
        } else {
            log::warn!("[report] meeting folder {fp} is unusable; using app-data fallback");
        }
    }

    // 2) Fallback under the app data directory.
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let dir = base.join("reports").join(sanitize_component(meeting_id));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create report dir failed: {e}"))?;
    let path = dir.join(&filename);
    std::fs::write(&path, html).map_err(|e| format!("write report file failed: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn russian_month(m: u32) -> &'static str {
    match m {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        _ => "декабря",
    }
}

/// Load a display title, Russian date string, and the meeting's storage folder (where the
/// report should be written). Falls back gracefully when the meeting row is missing.
async fn load_meeting(pool: &SqlitePool, meeting_id: &str) -> (String, String, Option<String>) {
    match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
        Ok(Some(m)) => {
            use chrono::Datelike;
            let dt = m.created_at.0;
            let date_str = format!("{} {} {}", dt.day(), russian_month(dt.month()), dt.year());
            (m.title, date_str, m.folder_path)
        }
        _ => ("Встреча".to_string(), String::new(), None),
    }
}

/// Run the full pipeline. Intended to be spawned via `tauri::async_runtime::spawn`.
pub async fn run_report_pipeline<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    report_id: String,
    meeting_id: String,
    model: String,
) {
    let token = register_token(&report_id);
    let start = Instant::now();

    // ---- transcript (hard requirement) ----
    let mut segments = match SpeakersRepository::meeting_transcript_segments(&pool, &meeting_id).await {
        Ok(s) => s,
        Err(e) => {
            fail(&app, &pool, &report_id, &meeting_id, &format!("Не удалось прочитать транскрипт: {e}")).await;
            return;
        }
    };
    if segments.is_empty() {
        fail(&app, &pool, &report_id, &meeting_id, "В этой встрече нет транскрипта.").await;
        return;
    }

    let (meeting_title, date_str, folder_path) = load_meeting(&pool, &meeting_id).await;

    // ---- privacy gate + credentials (hard requirements, before first network call) ----
    if let Err(e) = ensure_outbound_allowed(&pool, Purpose::Extract).await {
        fail(&app, &pool, &report_id, &meeting_id, &e.to_string()).await;
        return;
    }
    let client = match resolve_deepseek(&pool).await {
        Some(c) => c,
        None => {
            fail(
                &app,
                &pool,
                &report_id,
                &meeting_id,
                "DeepSeek не настроен — добавьте ключ в настройках",
            )
            .await;
            return;
        }
    };

    let mut failed: Vec<String> = Vec::new();

    // ---- stage 1: speakers (LLM + interactive; skipped when nothing is diarized) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 0).await;
    let roster = match SpeakersRepository::meeting_speakers(&pool, &meeting_id).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[report] failed to load meeting speakers: {e}");
            Vec::new()
        }
    };
    let mut speaker_suggestions: Vec<SpeakerSuggestion> = Vec::new();
    let mut speaker_decisions: Vec<SpeakerDecision> = Vec::new();
    if !roster.is_empty() {
        // Transcript labeled with stable speaker ids so guesses reference `id N`, not
        // display names (which may collide or be renumbered).
        let (dyn_now, labels_now, texts_now) = build_segment_views(&segments);
        let timed_now = dynamics::timeline(&dyn_now);
        let id_labels: Vec<String> = segments
            .iter()
            .map(|s| match s.speaker_id {
                Some(id) => format!("{} [id {id}]", speaker_label(s)),
                None => speaker_label(s),
            })
            .collect();
        let spk_transcript = prompts::truncate_transcript(&prompts::format_transcript(
            &timed_now, &id_labels, &texts_now,
        ));
        let roster_entries: Vec<(i64, String, i64)> = roster
            .iter()
            .map(|s| (s.id, s.display_name.clone(), s.segment_count))
            .collect();
        let (sys, usr) =
            prompts::speakers(&spk_transcript, &prompts::speaker_roster(&roster_entries));
        let guesses: Option<SpeakerGuesses> =
            run_stage(&client, &sys, &usr, "speakers", &mut failed).await;
        if token.is_cancelled() {
            finish_cancelled(&pool, &report_id).await;
            return;
        }

        // Pause for confirmation even without usable guesses — the user can still
        // rename and merge manually in the same dialog, using the excerpts below.
        speaker_suggestions = build_speaker_suggestions(&roster, guesses.as_ref());
        enrich_suggestions(
            &mut speaker_suggestions,
            &SpeakerContext {
                segments: &segments,
                timed: &timed_now,
                labels: &labels_now,
            },
            guesses.as_ref(),
        );
        let suggestions_json =
            serde_json::to_string(&speaker_suggestions).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) =
            AnalyticsReportsRepository::set_speakers_waiting(&pool, &report_id, &suggestions_json)
                .await
        {
            log::warn!("[report] failed to persist speaker suggestions for {report_id}: {e}");
        }
        let _ = app.emit(
            "analytics-report-speakers",
            json!({
                "report_id": report_id,
                "meeting_id": meeting_id,
                "speakers": speaker_suggestions,
            }),
        );

        match wait_for_speakers(&report_id, &token).await {
            Some(decisions) => speaker_decisions = decisions,
            None => {
                finish_cancelled(&pool, &report_id).await;
                return;
            }
        }
        let decisions_json =
            serde_json::to_string(&speaker_decisions).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) =
            AnalyticsReportsRepository::set_speakers_running(&pool, &report_id, &decisions_json)
                .await
        {
            log::warn!("[report] failed to persist speaker decisions for {report_id}: {e}");
        }

        if !speaker_decisions.is_empty()
            && apply_speaker_decisions(&pool, &meeting_id, &roster, &speaker_decisions).await
        {
            // Refresh the transcript/speaker UI outside the report dialog (same event +
            // payload shape the diarization run emits).
            let speaker_count = SpeakersRepository::meeting_speakers(&pool, &meeting_id)
                .await
                .map(|s| s.len() as i64)
                .unwrap_or(0);
            let assigned = segments.iter().filter(|s| s.speaker_id.is_some()).count() as i64;
            let _ = app.emit(
                "diarization-complete",
                json!({
                    "meeting_id": meeting_id,
                    "speaker_count": speaker_count,
                    "assigned_segments": assigned,
                }),
            );
            // Downstream stages must see the confirmed names and attributions.
            match SpeakersRepository::meeting_transcript_segments(&pool, &meeting_id).await {
                Ok(fresh) if !fresh.is_empty() => segments = fresh,
                Ok(_) => {}
                Err(e) => {
                    log::warn!("[report] failed to reload segments after speaker apply: {e}")
                }
            }
        }
    }
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 2: dynamics (local) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 1).await;
    let (dyn_segments, seg_labels, seg_texts) = build_segment_views(&segments);
    let timed = dynamics::timeline(&dyn_segments);
    let dyn_metrics = Dynamics::from_timed(&dyn_segments, &timed);

    let transcript = prompts::truncate_transcript(&prompts::format_transcript(
        &timed,
        &seg_labels,
        &seg_texts,
    ));

    // ---- stage 3: classify ----
    start_stage(&app, &pool, &report_id, &meeting_id, 2).await;
    let (sys, usr) = prompts::classify(&transcript);
    let classification: Option<Classification> =
        run_stage(&client, &sys, &usr, "classify", &mut failed).await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }
    let meeting_type = classification
        .as_ref()
        .map(|c| c.meeting_type.clone())
        .unwrap_or_default();

    // ---- stage 4: clarify (interactive) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 3).await;
    let classification_json =
        serde_json::to_string(&classification).unwrap_or_else(|_| "null".to_string());
    let (sys, usr) = prompts::clarify(&transcript, &classification_json);
    let clarify: Option<Clarify> = run_stage(&client, &sys, &usr, "clarify", &mut failed).await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // Present questions and wait for answers (unless there are none / stage soft-failed).
    let mut clarify_questions: Vec<ClarifyQuestion> = Vec::new();
    let mut clarify_answers: Vec<ClarifyAnswer> = Vec::new();
    if let Some(c) = &clarify {
        if !c.questions.is_empty() {
            // Guarantee the literal "другое" tap option on every question.
            clarify_questions = c.questions.clone();
            for q in &mut clarify_questions {
                if !q.options.iter().any(|o| o.trim() == "другое") {
                    q.options.push("другое".to_string());
                }
            }
            let questions_json =
                serde_json::to_string(&clarify_questions).unwrap_or_else(|_| "[]".to_string());
            if let Err(e) =
                AnalyticsReportsRepository::set_questions_waiting(&pool, &report_id, &questions_json)
                    .await
            {
                log::warn!("[report] failed to persist clarify questions for {report_id}: {e}");
            }
            let _ = app.emit(
                "analytics-report-questions",
                json!({
                    "report_id": report_id,
                    "meeting_id": meeting_id,
                    "questions": clarify_questions,
                }),
            );

            match wait_for_answers(&report_id, &token).await {
                Some(answers) => clarify_answers = answers,
                None => {
                    finish_cancelled(&pool, &report_id).await;
                    return;
                }
            }

            let answers_json =
                serde_json::to_string(&clarify_answers).unwrap_or_else(|_| "[]".to_string());
            if let Err(e) =
                AnalyticsReportsRepository::set_answers_running(&pool, &report_id, &answers_json)
                    .await
            {
                log::warn!("[report] failed to persist clarify answers for {report_id}: {e}");
            }
        }
    }

    // Confirmed clarifications are appended to every downstream extraction prompt.
    let answers_block = prompts::build_answers_block(&clarify_questions, &clarify_answers);

    // ---- stage 5: topics ----
    start_stage(&app, &pool, &report_id, &meeting_id, 4).await;
    let (sys, usr) = prompts::topics(&transcript, &meeting_type);
    let topics: Option<Topics> =
        run_stage(&client, &sys, &prompts::with_context(&usr, &answers_block), "topics", &mut failed)
            .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 6: decisions ----
    start_stage(&app, &pool, &report_id, &meeting_id, 5).await;
    let (sys, usr) = prompts::decisions(&transcript);
    let decisions: Option<prompts::Decisions> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "decisions",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 7: commitments ----
    start_stage(&app, &pool, &report_id, &meeting_id, 6).await;
    let (sys, usr) = prompts::commitments(&transcript);
    let commitments: Option<Commitments> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "commitments",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 8: threads + risks ----
    start_stage(&app, &pool, &report_id, &meeting_id, 7).await;
    let (sys, usr) = prompts::threads_risks(&transcript);
    let threads_risks: Option<ThreadsRisks> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "threads_risks",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 9: disagreements + concepts ----
    start_stage(&app, &pool, &report_id, &meeting_id, 8).await;
    let (sys, usr) = prompts::disagreements_concepts(&transcript);
    let disagreements_concepts: Option<DisagreementsConcepts> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "disagreements_concepts",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 10: numbers ----
    start_stage(&app, &pool, &report_id, &meeting_id, 9).await;
    let (sys, usr) = prompts::numbers(&transcript);
    let numbers: Option<Numbers> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "numbers",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 11: roles ----
    start_stage(&app, &pool, &report_id, &meeting_id, 10).await;
    let (sys, usr) = prompts::roles(&transcript);
    let roles: Option<Roles> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "roles",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 12: insights (over artifacts + fast facts, NOT the transcript) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 11).await;
    let artifacts_for_insights = json!({
        "classification": classification,
        "topics": topics,
        "decisions": decisions,
        "commitments": commitments,
        "threads_risks": threads_risks,
        "disagreements_concepts": disagreements_concepts,
        "numbers": numbers,
        "roles": roles,
    })
    .to_string();
    let fast = prompts::fast_facts(&dyn_metrics);
    let (sys, usr) = prompts::insights(&artifacts_for_insights, &fast);
    let insights: Option<Insights> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "insights",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 13: score + render (local) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 12).await;
    let empty_agenda = Vec::new();
    let agenda = topics.as_ref().map(|t| &t.agenda).unwrap_or(&empty_agenda);
    let empty_commitments = Vec::new();
    let commitments_vec = commitments
        .as_ref()
        .map(|c| &c.commitments)
        .unwrap_or(&empty_commitments);
    let open_len = threads_risks
        .as_ref()
        .map(|t| t.open_threads.len())
        .unwrap_or(0);
    let score = compute_score(agenda, commitments_vec, open_len);

    let generation_secs = start.elapsed().as_secs_f64();
    let html = render_report(&RenderInput {
        meeting_title: &meeting_title,
        date_str: &date_str,
        model: &model,
        generation_secs,
        total_stages: TOTAL_STAGES,
        dynamics: &dyn_metrics,
        timed: &timed,
        seg_labels: &seg_labels,
        seg_texts: &seg_texts,
        classification: classification.as_ref(),
        topics: topics.as_ref(),
        decisions: decisions.as_ref(),
        commitments: commitments.as_ref(),
        threads_risks: threads_risks.as_ref(),
        disagreements_concepts: disagreements_concepts.as_ref(),
        numbers: numbers.as_ref(),
        roles: roles.as_ref(),
        insights: insights.as_ref(),
        score: &score,
        clarify_questions: &clarify_questions,
        clarify_answers: &clarify_answers,
    });

    let html_path = match write_html(&app, &meeting_id, folder_path.as_deref(), &html) {
        Ok(p) => p,
        Err(e) => {
            fail(&app, &pool, &report_id, &meeting_id, &e).await;
            return;
        }
    };

    let artifacts_store = json!({
        "dynamics": dyn_metrics,
        "score": score,
        "speaker_suggestions": speaker_suggestions,
        "speaker_decisions": speaker_decisions,
        "classification": classification,
        "clarify_questions": clarify_questions,
        "clarify_answers": clarify_answers,
        "topics": topics,
        "decisions": decisions,
        "commitments": commitments,
        "threads_risks": threads_risks,
        "disagreements_concepts": disagreements_concepts,
        "numbers": numbers,
        "roles": roles,
        "insights": insights,
        "failed_stages": failed,
    })
    .to_string();

    if let Err(e) =
        AnalyticsReportsRepository::mark_completed(&pool, &report_id, &html_path, &artifacts_store)
            .await
    {
        fail(
            &app,
            &pool,
            &report_id,
            &meeting_id,
            &format!("Не удалось сохранить отчёт: {e}"),
        )
        .await;
        return;
    }

    cleanup_token(&report_id);
    log::info!(
        "[report] {report_id} completed in {:.1}s ({} stage(s) degraded)",
        generation_secs,
        failed.len()
    );
    let _ = app.emit(
        "analytics-report-complete",
        json!({ "report_id": report_id, "meeting_id": meeting_id, "html_path": html_path }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_fences_handles_plain_and_fenced_and_noisy() {
        assert_eq!(strip_json_fences("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_json_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(
            strip_json_fences("Вот ответ: {\"a\":1} — готово"),
            "{\"a\":1}"
        );
    }

    #[test]
    fn sanitize_component_strips_path_separators() {
        assert_eq!(sanitize_component("meeting-abc_123"), "meeting-abc_123");
        assert_eq!(sanitize_component("../../etc/passwd"), "______etc_passwd");
    }

    fn roster3() -> Vec<MeetingSpeaker> {
        vec![
            MeetingSpeaker {
                id: 1,
                display_name: "Speaker 1".into(),
                is_confirmed: false,
                segment_count: 40,
            },
            MeetingSpeaker {
                id: 2,
                display_name: "Speaker 2".into(),
                is_confirmed: false,
                segment_count: 10,
            },
            MeetingSpeaker {
                id: 3,
                display_name: "Аня".into(),
                is_confirmed: true,
                segment_count: 5,
            },
        ]
    }

    #[test]
    fn speaker_suggestions_sanitize_llm_guesses() {
        use crate::report::prompts::{SpeakerMergeGuess, SpeakerNameGuess};

        let guesses = SpeakerGuesses {
            names: vec![
                SpeakerNameGuess {
                    speaker_id: 1,
                    name: "Андрей".into(),
                    confidence: 1.7, // clamped
                    evidence: "меня зовут Андрей".into(),
                    seg: 3,
                },
                // Unknown id — dropped.
                SpeakerNameGuess {
                    speaker_id: 99,
                    name: "Призрак".into(),
                    ..Default::default()
                },
                // Blank name — dropped.
                SpeakerNameGuess {
                    speaker_id: 2,
                    name: "  ".into(),
                    ..Default::default()
                },
            ],
            merges: vec![
                // Self-merge and unknown ids inside the list are skipped, 2 -> 1 stays.
                SpeakerMergeGuess {
                    keep_id: 1,
                    merge_ids: vec![2, 1, 99],
                    reason: "одна манера речи".into(),
                },
                // keep_id 2 is already merged away — the whole group is dropped (no chains).
                SpeakerMergeGuess {
                    keep_id: 2,
                    merge_ids: vec![3],
                    reason: "цепочка".into(),
                },
            ],
        };

        let s = build_speaker_suggestions(&roster3(), Some(&guesses));
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].suggested_name.as_deref(), Some("Андрей"));
        assert_eq!(s[0].confidence, 1.0);
        assert!(s[0].merge_into.is_none());
        assert!(s[1].suggested_name.is_none());
        assert_eq!(s[1].merge_into, Some(1));
        assert_eq!(s[1].merge_reason.as_deref(), Some("одна манера речи"));
        assert!(s[2].merge_into.is_none(), "chained merge group must be dropped");
    }

    #[test]
    fn speaker_suggestions_without_guesses_list_the_whole_roster() {
        let s = build_speaker_suggestions(&roster3(), None);
        assert_eq!(s.len(), 3);
        assert!(s.iter().all(|x| x.suggested_name.is_none() && x.merge_into.is_none()));
        assert_eq!(s[2].current_name, "Аня");
        assert!(s[2].is_confirmed);
    }

    fn seg(text: &str, speaker_id: Option<i64>) -> TranscriptSpeakerSegment {
        TranscriptSpeakerSegment {
            text: text.to_string(),
            timestamp: String::new(),
            audio_start_time: None,
            speaker: None,
            speaker_id,
            display_name: speaker_id.map(|id| format!("Speaker {id}")),
        }
    }

    /// A 10-line conversation: speaker 1 and 2 alternate, 3 chimes in twice late.
    fn convo() -> Vec<TranscriptSpeakerSegment> {
        vec![
            seg("Всем привет, меня зовут Андрей, я сегодня веду встречу", Some(1)),
            seg("Привет, Андрей, я готова начинать обсуждение", Some(2)),
            seg("Отлично, тогда давайте по первому пункту повестки", Some(1)),
            seg("Да", Some(2)),
            seg("Нам нужно посчитать бюджет на следующий квартал внимательно", Some(1)),
            seg("Я подготовлю смету к пятнице и пришлю всем участникам", Some(2)),
            seg("Коллеги, извините что вклиниваюсь, у меня есть вопрос", Some(3)),
            seg("Конечно, спрашивайте, мы как раз обсуждаем бюджет", Some(1)),
            seg("Спасибо, вопрос про сроки поставки оборудования в регионы", Some(3)),
            seg("Хорошо, зафиксировали, обсудим это отдельно на следующей встрече", Some(1)),
        ]
    }

    fn ctx_for(segments: &[TranscriptSpeakerSegment]) -> (Vec<dynamics::TimedSegment>, Vec<String>) {
        let (dyn_segs, labels, _) = build_segment_views(segments);
        (dynamics::timeline(&dyn_segs), labels)
    }

    #[test]
    fn samples_spread_across_the_meeting_and_skip_filler() {
        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        let s1 = ctx.samples(1, 4);
        assert_eq!(s1.len(), 4);
        // Chronological, spanning the first and last line of speaker 1.
        assert_eq!(s1[0].seg, 0);
        assert_eq!(s1[3].seg, 9);
        assert!(s1.iter().all(|l| l.speaker_id == Some(1)));

        // Speaker 2's "Да" is filler and must lose to the substantive lines.
        let s2 = ctx.samples(2, 4);
        assert_eq!(s2.len(), 2, "only two substantive lines exist");
        assert!(!s2.iter().any(|l| l.text == "Да"));

        // Fewer turns than requested samples is fine.
        assert_eq!(ctx.samples(3, 4).len(), 2);
        assert!(ctx.samples(99, 4).is_empty());
    }

    #[test]
    fn samples_fall_back_to_filler_when_nothing_longer_exists() {
        let segments = vec![seg("Да", Some(7)), seg("Угу", Some(7))];
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };
        let s = ctx.samples(7, 4);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].text, "Да");
    }

    #[test]
    fn evidence_window_highlights_the_quoted_line_with_context() {
        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        let i = ctx
            .locate_evidence(0, "меня зовут Андрей", 1)
            .expect("quote is in segment 0");
        assert_eq!(i, 0);
        let w = ctx.window(i, 2, 2);
        // Clamped at the start; highlight lands on the evidence line.
        assert_eq!(w.len(), 3);
        assert!(w[0].highlight);
        assert_eq!(w[2].seg, 2);
        assert_eq!(w[1].label, "Speaker 2");
    }

    #[test]
    fn evidence_lookup_survives_a_wrong_reported_index() {
        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        // Model reported the wrong line but quoted accurately -> text search wins.
        assert_eq!(ctx.locate_evidence(5, "меня зовут Андрей", 1), Some(0));
        // Out-of-range index, quote present -> found by search.
        assert_eq!(ctx.locate_evidence(999, "меня зовут Андрей", 1), Some(0));
        // Paraphrased quote -> falls back to the reported (in-range) index.
        assert_eq!(ctx.locate_evidence(4, "этого текста тут нет", 1), Some(4));
        // Nothing usable at all.
        assert_eq!(ctx.locate_evidence(-1, "этого текста тут нет", 1), None);
    }

    #[test]
    fn pair_excerpt_picks_where_both_speakers_are_closest() {
        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        // Speakers 1 and 3 are adjacent at segments 6-7.
        let p = ctx.pair(1, 3, 6);
        assert!(!p.is_empty());
        let segs: Vec<i64> = p.iter().map(|l| l.seg).collect();
        assert!(segs.contains(&6) && segs.contains(&7), "got {segs:?}");
        assert!(p.len() <= 6);

        // A speaker that never appears has no comparison excerpt.
        assert!(ctx.pair(1, 42, 6).is_empty());
    }

    #[test]
    fn enrichment_fills_stats_samples_and_contexts() {
        use crate::report::prompts::{SpeakerMergeGuess, SpeakerNameGuess};

        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        let roster = vec![
            MeetingSpeaker { id: 1, display_name: "Speaker 1".into(), is_confirmed: false, segment_count: 5 },
            MeetingSpeaker { id: 2, display_name: "Speaker 2".into(), is_confirmed: false, segment_count: 3 },
            MeetingSpeaker { id: 3, display_name: "Speaker 3".into(), is_confirmed: false, segment_count: 2 },
        ];
        let guesses = SpeakerGuesses {
            names: vec![SpeakerNameGuess {
                speaker_id: 1,
                name: "Андрей".into(),
                confidence: 0.95,
                evidence: "меня зовут Андрей".into(),
                seg: 0,
            }],
            merges: vec![SpeakerMergeGuess {
                keep_id: 1,
                merge_ids: vec![3],
                reason: "одна манера речи".into(),
            }],
        };

        let mut suggestions = build_speaker_suggestions(&roster, Some(&guesses));
        enrich_suggestions(&mut suggestions, &ctx, Some(&guesses));

        // Every speaker gets recognition samples and stats.
        assert!(suggestions.iter().all(|s| !s.samples.is_empty()));
        assert!(suggestions.iter().all(|s| !s.first_seen.is_empty()));
        let total: f32 = suggestions.iter().map(|s| s.talk_share).sum();
        assert!((total - 1.0).abs() < 0.01, "talk shares sum to {total}");
        assert!(suggestions[0].talk_share > suggestions[2].talk_share);

        // The named speaker gets the dialogue around the evidence; others don't.
        assert!(!suggestions[0].evidence_context.is_empty());
        assert!(suggestions[0].evidence_context.iter().any(|l| l.highlight));
        assert!(suggestions[1].evidence_context.is_empty());

        // The merge candidate (speaker 3 -> 1) gets the two-speaker comparison.
        assert_eq!(suggestions[2].merge_into, Some(1));
        let ctx_ids: Vec<Option<i64>> = suggestions[2]
            .merge_context
            .iter()
            .map(|l| l.speaker_id)
            .collect();
        assert!(ctx_ids.contains(&Some(1)) && ctx_ids.contains(&Some(3)), "got {ctx_ids:?}");
        assert!(suggestions[0].merge_context.is_empty(), "the kept speaker has no merge excerpt");
    }

    #[test]
    fn long_lines_are_clipped_on_char_boundaries() {
        let long = "я".repeat(LINE_CHARS + 50);
        let segments = vec![seg(&long, Some(1))];
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };
        let line = ctx.line(0, false);
        assert_eq!(line.text.chars().count(), LINE_CHARS + 1); // + ellipsis
        assert!(line.text.ends_with('…'));
        assert_eq!(clip("коротко", LINE_CHARS), "коротко");
    }

    #[test]
    fn pair_excerpt_includes_both_turns_even_when_far_apart() {
        // Speaker 1 speaks once at the start, speaker 2 once at the end, with speaker 3
        // filling the long middle — the merge-candidate case where the two never speak
        // near each other. The old clamp truncated the excerpt before the second turn.
        let mut segments = vec![seg("Здравствуйте, начнём совещание прямо сейчас", Some(1))];
        for _ in 0..10 {
            segments.push(seg("Довольно длинная реплика третьего участника встречи", Some(3)));
        }
        segments.push(seg("Спасибо, у меня короткое дополнение по срокам", Some(2)));
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        let last = (segments.len() - 1) as i64;
        let p = ctx.pair(1, 2, PAIR_LINES);
        let segs: Vec<i64> = p.iter().map(|l| l.seg).collect();
        assert!(segs.contains(&0), "first speaker's turn missing: {segs:?}");
        assert!(segs.contains(&last), "second speaker's turn missing: {segs:?}");
        assert!(p.len() <= PAIR_LINES, "excerpt exceeds budget: {segs:?}");
        assert!(p.iter().any(|l| l.speaker_id == Some(1)), "speaker 1 absent");
        assert!(p.iter().any(|l| l.speaker_id == Some(2)), "speaker 2 absent");
    }

    #[test]
    fn evidence_seg_matches_the_first_guess_used_for_the_name() {
        use crate::report::prompts::SpeakerNameGuess;

        let segments = convo();
        let (timed, labels) = ctx_for(&segments);
        let ctx = SpeakerContext { segments: &segments, timed: &timed, labels: &labels };

        // The model emitted TWO guesses for speaker 1. The FIRST drives the displayed
        // name, so the evidence line must come from the FIRST guess's seg too. The first
        // guess's quote is paraphrased (not verbatim), so `locate_evidence` cannot rescue
        // a wrong seg via text search — it falls back to the reported seg, exposing any
        // first/last-wins mismatch between the name and the evidence index.
        let guesses = SpeakerGuesses {
            names: vec![
                SpeakerNameGuess {
                    speaker_id: 1,
                    name: "Андрей".into(),
                    confidence: 0.9,
                    evidence: "он назвал своё имя в начале".into(), // paraphrase
                    seg: 0,
                },
                SpeakerNameGuess {
                    speaker_id: 1,
                    name: "Борис".into(),
                    confidence: 0.8,
                    evidence: "что-то другое".into(),
                    seg: 5,
                },
            ],
            merges: vec![],
        };

        let mut s = build_speaker_suggestions(&roster3(), Some(&guesses));
        assert_eq!(s[0].suggested_name.as_deref(), Some("Андрей"));
        enrich_suggestions(&mut s, &ctx, Some(&guesses));

        let highlighted = s[0].evidence_context.iter().find(|l| l.highlight).map(|l| l.seg);
        assert_eq!(
            highlighted,
            Some(0),
            "evidence must anchor on the first guess's seg (0), not the last guess's (5)"
        );
        assert!(s[0].evidence_context.iter().all(|l| l.seg != 5));
    }

    /// In-memory pool with the schema subset the apply path touches (same shape as the
    /// speaker repository's GC tests).
    async fn apply_test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory db");
        sqlx::query(
            "CREATE TABLE speakers (
                id INTEGER PRIMARY KEY,
                display_name TEXT NOT NULL,
                voice_embedding BLOB,
                is_confirmed INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                speaker_id INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, name, confirmed) in [(1, "Speaker 1", 0), (2, "Speaker 2", 0), (3, "Аня", 1)] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, ?)")
                .bind(id)
                .bind(name)
                .bind(confirmed)
                .execute(&pool)
                .await
                .unwrap();
        }
        for (tid, sid) in [("a", 1), ("b", 2), ("c", 3)] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES (?, 'm1', ?)")
                .bind(tid)
                .bind(sid)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn apply_decisions_merges_renames_and_drops_invalid() {
        let pool = apply_test_pool().await;
        let decisions = vec![
            // Valid merge: 2 -> 1.
            SpeakerDecision {
                speaker_id: 2,
                display_name: None,
                merge_into: Some(1),
            },
            // Valid rename.
            SpeakerDecision {
                speaker_id: 1,
                display_name: Some("Андрей".into()),
                merge_into: None,
            },
            // Invalid: merge into a speaker that is itself merged (chain) — dropped.
            SpeakerDecision {
                speaker_id: 3,
                display_name: None,
                merge_into: Some(2),
            },
            // Invalid: unknown speaker — dropped.
            SpeakerDecision {
                speaker_id: 99,
                display_name: Some("Призрак".into()),
                merge_into: None,
            },
        ];

        let changed = apply_speaker_decisions(&pool, "m1", &roster3(), &decisions).await;
        assert!(changed);

        let attributions: Vec<(String, i64)> =
            sqlx::query_as("SELECT id, speaker_id FROM transcripts ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            attributions,
            vec![("a".into(), 1), ("b".into(), 1), ("c".into(), 3)]
        );

        let speakers: Vec<(i64, String, i64)> =
            sqlx::query_as("SELECT id, display_name, is_confirmed FROM speakers ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        // Speaker 2 was merged away and GC'd; speaker 1 renamed + confirmed; 3 untouched.
        assert_eq!(
            speakers,
            vec![(1, "Андрей".into(), 1), (3, "Аня".into(), 1)]
        );
    }

    #[tokio::test]
    async fn apply_decisions_with_unchanged_names_changes_nothing() {
        let pool = apply_test_pool().await;
        let decisions = vec![SpeakerDecision {
            speaker_id: 1,
            display_name: Some("Speaker 1".into()), // equals current -> no rename
            merge_into: None,
        }];
        let changed = apply_speaker_decisions(&pool, "m1", &roster3(), &decisions).await;
        assert!(!changed);
        let confirmed: i64 =
            sqlx::query_scalar("SELECT is_confirmed FROM speakers WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(confirmed, 0, "untouched speaker must stay unconfirmed");
    }
}
