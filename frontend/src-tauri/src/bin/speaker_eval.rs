//! speaker_eval — dev-only measurement harness for speaker identification.
//!
//! Speaker-ID thresholds in this codebase were chosen by inspection, not by
//! measurement, and drifted badly out of calibration (see
//! docs/superpowers/specs/2026-07-28-speaker-identification-design.md). This
//! binary is the missing feedback loop: it reports the numbers that any change
//! to enrollment, matching, or thresholds must be judged against.
//!
//! It deliberately calls the REAL `diarization` modules rather than
//! reimplementing them, so what it measures is the shipping code path.
//!
//! Not built by default. Run with:
//!   cargo run --features dev-eval --bin speaker_eval -- --help
//!
//! The two reports:
//!   * profile store  — needs only the database. Margin, EER, clash matrix.
//!   * audio          — needs recordings + the CAM++ model. Establishes the
//!                      label-free "same speaker" anchor and tells label noise
//!                      apart from genuine voice drift.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

use app_lib::audio::decoder::decode_audio_file;
use app_lib::database::repositories::speaker_profile::SpeakerProfilesRepository;
use app_lib::diarization::clustering::cosine_similarity;
use app_lib::diarization::embedding::EmbeddingExtractor;
use app_lib::diarization::flagging::{flag_confusable_profiles, CONFUSABLE_THRESHOLD};
use app_lib::diarization::normalize::{center_normalized, cohort_mean, MIN_PROFILES_FOR_CENTERING};

const SR: usize = 16_000;

#[derive(Parser, Debug)]
#[command(
    name = "speaker_eval",
    about = "Measure speaker identification quality (dev tool, not shipped)"
)]
struct Args {
    /// Path to meeting_minutes.sqlite. Defaults to the macOS app data location.
    #[arg(long)]
    db: Option<PathBuf>,

    /// Path to wespeaker_en_voxceleb_CAM++.onnx. Required for the audio report.
    #[arg(long)]
    model: Option<PathBuf>,

    /// Run the audio report against meetings whose folder_path contains this
    /// substring. Without it, only the profile-store report runs.
    #[arg(long)]
    meeting: Option<String>,

    /// Minimum labelled segment duration, in seconds, for the audio report.
    #[arg(long, default_value_t = 3.0)]
    min_secs: f64,

    /// Cap on segments embedded per meeting, to keep runs quick.
    #[arg(long, default_value_t = 400)]
    max_segments: usize,
}

// ---------------------------------------------------------------- statistics

/// Summary of a score distribution. `p(q)` is the q-quantile.
struct Dist {
    n: usize,
    mean: f32,
    min: f32,
    max: f32,
    sorted: Vec<f32>,
}

impl Dist {
    fn new(mut v: Vec<f32>) -> Self {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = v.len();
        let mean = if n == 0 { 0.0 } else { v.iter().sum::<f32>() / n as f32 };
        Self {
            n,
            mean,
            min: v.first().copied().unwrap_or(0.0),
            max: v.last().copied().unwrap_or(0.0),
            sorted: v,
        }
    }

    fn p(&self, q: f32) -> f32 {
        if self.sorted.is_empty() {
            return 0.0;
        }
        let i = ((q * self.sorted.len() as f32) as usize).min(self.sorted.len() - 1);
        self.sorted[i]
    }

    fn line(&self, label: &str) -> String {
        format!(
            "{label:<28} n={:<6} mean={:+.4} p10={:+.4} median={:+.4} p90={:+.4} min={:+.4} max={:+.4}",
            self.n,
            self.mean,
            self.p(0.10),
            self.p(0.50),
            self.p(0.90),
            self.min,
            self.max
        )
    }
}

/// Equal-error rate between a same-speaker and a different-speaker score
/// distribution, plus the threshold that achieves it.
///
/// Returns `(threshold, false_reject, false_accept)`. False-reject is the
/// fraction of genuine same-speaker pairs that a threshold would turn away;
/// false-accept is the fraction of different-speaker pairs it would merge.
fn equal_error_rate(same: &[f32], diff: &[f32]) -> Option<(f32, f32, f32)> {
    if same.is_empty() || diff.is_empty() {
        return None;
    }
    let mut best: Option<(f32, f32, f32)> = None;
    let mut t = -1.0f32;
    while t <= 1.0 {
        let fr = same.iter().filter(|s| **s < t).count() as f32 / same.len() as f32;
        let fa = diff.iter().filter(|s| **s >= t).count() as f32 / diff.len() as f32;
        if best.map_or(true, |(_, bfr, bfa)| (fr - fa).abs() < (bfr - bfa).abs()) {
            best = Some((t, fr, fa));
        }
        t += 0.002;
    }
    best
}

fn error_rates_at(same: &[f32], diff: &[f32], t: f32) -> (f32, f32) {
    let fr = same.iter().filter(|s| **s < t).count() as f32 / same.len().max(1) as f32;
    let fa = diff.iter().filter(|s| **s >= t).count() as f32 / diff.len().max(1) as f32;
    (fr, fa)
}

fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Mean cosine over every pair within a group — how internally consistent a
/// set of embeddings is. Compare against the same-speaker anchor from the
/// audio report; well below it means the group holds more than one person.
fn coherence(vs: &[Vec<f32>]) -> f32 {
    let mut acc = 0.0f32;
    let mut n = 0usize;
    for i in 0..vs.len() {
        for j in (i + 1)..vs.len() {
            acc += cosine_similarity(&vs[i], &vs[j]);
            n += 1;
        }
    }
    if n == 0 {
        f32::NAN
    } else {
        acc / n as f32
    }
}

// ------------------------------------------------------- profile-store report

async fn profile_report(pool: &SqlitePool) -> Result<()> {
    let profiles = SpeakerProfilesRepository::list_with_exemplars(pool)
        .await
        .context("loading profiles with exemplars")?;

    println!("\n{}", "=".repeat(78));
    println!("PROFILE STORE");
    println!("{}", "=".repeat(78));

    if profiles.len() < 2 {
        println!("  fewer than 2 profiles with exemplars — nothing to compare");
        return Ok(());
    }

    let total_ex: usize = profiles.iter().map(|p| p.exemplars.len()).sum();
    println!("  profiles: {}   exemplars: {}", profiles.len(), total_ex);
    println!(
        "  centering active: {}  (needs >= {} profiles)",
        profiles.len() >= MIN_PROFILES_FOR_CENTERING,
        MIN_PROFILES_FOR_CENTERING
    );
    println!("\n  {:<16} {:>9} {:>12}", "name", "exemplars", "coherence");
    for p in &profiles {
        let c = coherence(&p.exemplars);
        let cs = if c.is_nan() { "     n/a".to_string() } else { format!("{c:+.4}") };
        println!("  {:<16} {:>9} {:>12}", p.name, p.exemplars.len(), cs);
    }

    // Exact and near-duplicate exemplars shared across different names are
    // data corruption, not similarity — surface them separately.
    println!("\n  cross-profile duplicate exemplars (> 0.99 cosine):");
    let mut dupes = 0usize;
    for i in 0..profiles.len() {
        for j in (i + 1)..profiles.len() {
            for a in &profiles[i].exemplars {
                for b in &profiles[j].exemplars {
                    let s = cosine_similarity(a, b);
                    if s > 0.99 {
                        dupes += 1;
                        println!(
                            "    {:.6}  {} <-> {}{}",
                            s,
                            profiles[i].name,
                            profiles[j].name,
                            if a == b { "   [byte-identical]" } else { "" }
                        );
                    }
                }
            }
        }
    }
    if dupes == 0 {
        println!("    none");
    }

    // Raw-space separation.
    let mut same_raw = Vec::new();
    let mut diff_raw = Vec::new();
    for i in 0..profiles.len() {
        for a in 0..profiles[i].exemplars.len() {
            for b in (a + 1)..profiles[i].exemplars.len() {
                same_raw.push(cosine_similarity(&profiles[i].exemplars[a], &profiles[i].exemplars[b]));
            }
        }
        for j in (i + 1)..profiles.len() {
            for a in &profiles[i].exemplars {
                for b in &profiles[j].exemplars {
                    diff_raw.push(cosine_similarity(a, b));
                }
            }
        }
    }

    println!("\n  RAW cosine");
    println!("  {}", Dist::new(same_raw.clone()).line("    same speaker"));
    println!("  {}", Dist::new(diff_raw.clone()).line("    different speakers"));
    let raw_margin = Dist::new(same_raw.clone()).mean - Dist::new(diff_raw.clone()).mean;
    println!("    margin = {raw_margin:+.4}");

    // Centered space — mirrors what the matcher does once enough profiles exist.
    let cohort: Vec<&[f32]> = profiles
        .iter()
        .flat_map(|p| p.exemplars.iter().map(|e| e.as_slice()))
        .collect();
    let mean = cohort_mean(&cohort).ok_or_else(|| anyhow!("cohort mean unavailable"))?;

    // `cohort_mean` deliberately returns an UN-normalized vector (its magnitude
    // is itself diagnostic), while `cosine_similarity` is a bare dot product
    // that assumes both sides are unit length. Feeding the raw mean in would
    // report |mean| * cos rather than cos, understating anisotropy — so
    // normalize a copy purely for this measurement.
    let mut mean_unit = mean.clone();
    l2_normalize(&mut mean_unit);
    let anis: Vec<f32> = cohort.iter().map(|e| cosine_similarity(e, &mean_unit)).collect();
    let centered: Vec<Vec<Vec<f32>>> = profiles
        .iter()
        .map(|p| p.exemplars.iter().map(|e| center_normalized(e, &mean)).collect())
        .collect();

    let mut same_c = Vec::new();
    let mut diff_c = Vec::new();
    for i in 0..centered.len() {
        for a in 0..centered[i].len() {
            for b in (a + 1)..centered[i].len() {
                same_c.push(cosine_similarity(&centered[i][a], &centered[i][b]));
            }
        }
        for j in (i + 1)..centered.len() {
            for a in &centered[i] {
                for b in &centered[j] {
                    diff_c.push(cosine_similarity(a, b));
                }
            }
        }
    }

    println!("\n  anisotropy: mean cos(exemplar, cohort mean) = {:+.4}", Dist::new(anis).mean);
    println!("  (near 1.0 means one shared direction dominates every embedding)");
    println!("\n  CENTERED cosine");
    println!("  {}", Dist::new(same_c.clone()).line("    same speaker"));
    println!("  {}", Dist::new(diff_c.clone()).line("    different speakers"));
    let c_margin = Dist::new(same_c.clone()).mean - Dist::new(diff_c.clone()).mean;
    println!("    margin = {c_margin:+.4}");

    if let Some((t, fr, fa)) = equal_error_rate(&same_c, &diff_c) {
        println!(
            "\n  EER = {:.1}% at threshold {:+.3}   (usable systems are under 5%)",
            (fr + fa) / 2.0 * 100.0,
            t
        );
    }

    println!("\n  threshold sweep (centered space)");
    println!("  {:>8} {:>26} {:>26}", "thr", "false-reject (same missed)", "false-accept (wrong merge)");
    for t in [0.20f32, 0.30, 0.40, 0.50, 0.55, 0.60, 0.70] {
        let (fr, fa) = error_rates_at(&same_c, &diff_c, t);
        println!("  {t:>8.2} {:>25.1}% {:>25.1}%", fr * 100.0, fa * 100.0);
    }

    // Clash flagging: the shipped max-linkage statistic against a
    // count-robust mean-linkage alternative on the same data.
    let input: Vec<(String, Vec<Vec<f32>>)> = profiles
        .iter()
        .map(|p| (p.name.clone(), p.exemplars.clone()))
        .collect();
    let flags = flag_confusable_profiles(&input);
    println!(
        "\n  clash flags from the shipped detector (mean-linkage, thr {CONFUSABLE_THRESHOLD}):"
    );
    if flags.is_empty() {
        println!("    none");
    }
    for f in &flags {
        println!("    {:<16} may clash with {:<16} score={:+.4}", f.name, f.confused_with, f.score);
    }

    // The statistic this replaced, for comparison. Max-linkage rises with the
    // number of exemplars a profile happens to hold (see the table below), so
    // it manufactures warnings as the user records more meetings.
    println!("\n  the same pairs under the OLD max-linkage statistic:");
    let mut max_flagged = 0usize;
    for i in 0..centered.len() {
        for j in (i + 1)..centered.len() {
            let mut best = f32::MIN;
            for a in &centered[i] {
                for b in &centered[j] {
                    best = best.max(cosine_similarity(a, b));
                }
            }
            if best >= CONFUSABLE_THRESHOLD {
                max_flagged += 1;
                println!("    {:+.4}  {} <-> {}", best, profiles[i].name, profiles[j].name);
            }
        }
    }
    println!(
        "    pairs flagged: mean-linkage {} vs max-linkage {}",
        flags.len() / 2,
        max_flagged
    );
    println!(
        "    (max-linkage also catches genuine duplicates, which are now reported\n     \
         separately and precisely in the duplicate section above)"
    );

    // Max-linkage inflates with the number of comparisons; show it directly.
    println!("\n  max-linkage score vs number of exemplar comparisons:");
    let mut buckets: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for i in 0..centered.len() {
        for j in (i + 1)..centered.len() {
            let mut best = f32::MIN;
            for a in &centered[i] {
                for b in &centered[j] {
                    best = best.max(cosine_similarity(a, b));
                }
            }
            buckets.entry(centered[i].len() * centered[j].len()).or_default().push(best);
        }
    }
    for (k, v) in buckets {
        let d = Dist::new(v);
        println!("    {k:>4} comparisons ({:>2} pairs): mean max-score = {:+.4}", d.n, d.mean);
    }

    Ok(())
}

// ----------------------------------------------------------- audio report

struct LabelledSegment {
    speaker: String,
    start: f64,
    end: f64,
}

async fn labelled_segments(
    pool: &SqlitePool,
    folder_like: &str,
    min_secs: f64,
) -> Result<(String, Vec<LabelledSegment>)> {
    let row = sqlx::query(
        "SELECT id, folder_path FROM meetings \
         WHERE folder_path LIKE ? AND folder_path IS NOT NULL LIMIT 1",
    )
    .bind(format!("%{folder_like}%"))
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("no meeting whose folder_path contains {folder_like:?}"))?;

    let meeting_id: String = row.try_get("id")?;
    let folder: String = row.try_get("folder_path")?;

    let rows = sqlx::query(
        "SELECT speaker, audio_start_time, audio_end_time FROM transcripts \
         WHERE meeting_id = ? AND speaker IS NOT NULL \
           AND audio_start_time IS NOT NULL AND audio_end_time IS NOT NULL \
           AND audio_end_time - audio_start_time >= ? \
         ORDER BY audio_start_time",
    )
    .bind(&meeting_id)
    .bind(min_secs)
    .fetch_all(pool)
    .await?;

    let segs = rows
        .into_iter()
        .filter_map(|r| {
            Some(LabelledSegment {
                speaker: r.try_get("speaker").ok()?,
                start: r.try_get("audio_start_time").ok()?,
                end: r.try_get("audio_end_time").ok()?,
            })
        })
        .collect();

    Ok((folder, segs))
}

fn slice(audio: &[f32], start_s: f64, end_s: f64) -> &[f32] {
    let a = ((start_s * SR as f64) as usize).min(audio.len());
    let b = ((end_s * SR as f64) as usize).min(audio.len());
    if b > a {
        &audio[a..b]
    } else {
        &[]
    }
}

async fn audio_report(
    pool: &SqlitePool,
    model: &Path,
    folder_like: &str,
    min_secs: f64,
    max_segments: usize,
) -> Result<()> {
    let (folder, segs) = labelled_segments(pool, folder_like, min_secs).await?;
    println!("\n{}", "=".repeat(78));
    println!("AUDIO REPORT — {folder}");
    println!("{}", "=".repeat(78));
    println!("  labelled segments >= {min_secs}s: {}", segs.len());
    if segs.is_empty() {
        return Ok(());
    }

    let audio_path = Path::new(&folder).join("audio.mp4");
    if !audio_path.exists() {
        println!("  no audio.mp4 in that folder — skipping");
        return Ok(());
    }
    let decoded = decode_audio_file(&audio_path).map_err(|e| anyhow!("decode failed: {e}"))?;
    let audio = decoded.to_whisper_format();
    println!("  decoded {:.1}s at {SR} Hz mono", audio.len() as f64 / SR as f64);

    let mut extractor =
        EmbeddingExtractor::new(model).map_err(|e| anyhow!("loading CAM++ model: {e}"))?;

    // ---- anchor: two halves of ONE utterance are necessarily the same speaker.
    // This is the only ground truth available that does not depend on the
    // labels being correct, so every other number is read against it.
    let mut within_utterance = Vec::new();
    for s in segs.iter().filter(|s| s.end - s.start >= 6.0).take(max_segments) {
        let seg = slice(&audio, s.start, s.end);
        if seg.len() < 6 * SR {
            continue;
        }
        let mid = seg.len() / 2;
        if let (Ok(a), Ok(b)) = (extractor.compute(&seg[..mid]), extractor.compute(&seg[mid..])) {
            within_utterance.push(cosine_similarity(&a, &b));
        }
    }
    let anchor = Dist::new(within_utterance);
    println!("\n  ANCHOR — same utterance split in half (guaranteed same speaker)");
    println!("  {}", anchor.line("    within-utterance"));
    println!("    a healthy pipeline lands here around 0.80-0.95");

    // ---- per-segment embeddings, for the label-quality checks
    let mut embs: Vec<(String, f64, Vec<f32>)> = Vec::new();
    for s in segs.iter().take(max_segments) {
        let seg = slice(&audio, s.start, s.end);
        if seg.len() < (min_secs * SR as f64) as usize {
            continue;
        }
        if let Ok(v) = extractor.compute(seg) {
            embs.push((s.speaker.clone(), s.start, v));
        }
    }
    println!("\n  embedded {} labelled segments", embs.len());

    let mut per_label: HashMap<&str, usize> = HashMap::new();
    for (sp, _, _) in &embs {
        *per_label.entry(sp.as_str()).or_default() += 1;
    }
    let mut counts: Vec<_> = per_label.iter().collect();
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!(
        "  per label: {}",
        counts.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")
    );

    // ---- label noise vs voice drift.
    // If same-label similarity decays as the gap grows, the voice/channel is
    // drifting (a modelling problem). If it is flat and sits well below the
    // anchor, the labels are wrong (a labelling problem).
    const GAPS: [(f64, f64, &str); 5] = [
        (0.0, 30.0, "<30s"),
        (30.0, 120.0, "30s-2m"),
        (120.0, 300.0, "2-5m"),
        (300.0, 900.0, "5-15m"),
        (900.0, f64::INFINITY, ">15m"),
    ];
    let mut same_by_gap: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
    let mut diff_by_gap: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
    let mut same_all = Vec::new();
    let mut diff_all = Vec::new();
    for i in 0..embs.len() {
        for j in (i + 1)..embs.len() {
            let s = cosine_similarity(&embs[i].2, &embs[j].2);
            let gap = (embs[j].1 - embs[i].1).abs();
            let tag = GAPS.iter().find(|(lo, hi, _)| gap >= *lo && gap < *hi).map(|(_, _, t)| *t);
            if embs[i].0 == embs[j].0 {
                same_all.push(s);
                if let Some(t) = tag {
                    same_by_gap.entry(t).or_default().push(s);
                }
            } else {
                diff_all.push(s);
                if let Some(t) = tag {
                    diff_by_gap.entry(t).or_default().push(s);
                }
            }
        }
    }

    println!("\n  same-label similarity by time gap (flat => label noise, decaying => drift)");
    for (_, _, tag) in GAPS {
        if let Some(v) = same_by_gap.get(tag) {
            let d = Dist::new(v.clone());
            println!("    gap {tag:<8} n={:<6} mean={:+.4}", d.n, d.mean);
        }
    }
    println!("  different-label similarity by time gap (control)");
    for (_, _, tag) in GAPS {
        if let Some(v) = diff_by_gap.get(tag) {
            let d = Dist::new(v.clone());
            println!("    gap {tag:<8} n={:<6} mean={:+.4}", d.n, d.mean);
        }
    }

    let near = same_by_gap.get("<30s").map(|v| Dist::new(v.clone()).mean);
    let far = same_by_gap.get(">15m").map(|v| Dist::new(v.clone()).mean);
    if let (Some(n), Some(f)) = (near, far) {
        println!("\n    decay across the meeting = {:+.4}", n - f);
        println!("    (near zero rules out voice drift)");
    }

    let same_d = Dist::new(same_all.clone());
    let diff_d = Dist::new(diff_all.clone());
    println!("\n  {}", same_d.line("  same label"));
    println!("  {}", diff_d.line("  different label"));
    println!("    margin = {:+.4}", same_d.mean - diff_d.mean);
    if !anchor.sorted.is_empty() {
        println!(
            "    label-noise gap = anchor {:+.4} - same-label {:+.4} = {:+.4}",
            anchor.mean,
            same_d.mean,
            anchor.mean - same_d.mean
        );
        println!("    (a large gap means segments sharing a name are not the same person)");
    }
    if let Some((t, fr, fa)) = equal_error_rate(&same_all, &diff_all) {
        println!(
            "\n  EER against the stored labels = {:.1}% at threshold {:+.3}",
            (fr + fa) / 2.0 * 100.0,
            t
        );
        println!("  (this measures agreement with the labels, which may themselves be wrong)");
    }

    // ---- per-label coherence, read against the anchor
    println!("\n  per-label coherence (compare to the anchor above)");
    let mut by_label: BTreeMap<String, Vec<Vec<f32>>> = BTreeMap::new();
    for (sp, _, v) in &embs {
        by_label.entry(sp.clone()).or_default().push(v.clone());
    }
    for (name, vs) in &by_label {
        if vs.len() < 2 {
            continue;
        }
        println!("    {:<16} n={:<5} coherence={:+.4}", name, vs.len(), coherence(vs));
    }

    Ok(())
}

// ------------------------------------------------------------------- main

fn default_db() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join("Library/Application Support/com.meetily.ai/meeting_minutes.sqlite");
    p.exists().then_some(p)
}

fn default_model() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(
        "Library/Application Support/com.meetily.ai/models/diarization/wespeaker_en_voxceleb_CAM++.onnx",
    );
    p.exists().then_some(p)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let db = args
        .db
        .or_else(default_db)
        .ok_or_else(|| anyhow!("could not locate the database; pass --db"))?;
    println!("database: {}", db.display());

    // Read-only: this tool must never mutate a user's profile store.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}?mode=ro", db.display()))
        .await
        .with_context(|| format!("opening {} read-only", db.display()))?;

    profile_report(&pool).await?;

    if let Some(meeting) = args.meeting.as_deref() {
        let model = args
            .model
            .or_else(default_model)
            .ok_or_else(|| anyhow!("the audio report needs the CAM++ model; pass --model"))?;
        println!("\nmodel: {}", model.display());
        audio_report(&pool, &model, meeting, args.min_secs, args.max_segments).await?;
    } else {
        println!("\n(pass --meeting <substring> to also run the audio report)");
    }

    Ok(())
}
