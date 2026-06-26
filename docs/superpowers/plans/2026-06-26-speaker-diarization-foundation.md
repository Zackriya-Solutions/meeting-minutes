# Speaker Diarization Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first shippable foundation for speaker diarization: persisted settings, transcript speaker metadata, deterministic interval alignment, transcript UI labels, and post-call review-ready data structures.

**Architecture:** Keep transcription and recording behavior unchanged. Add a focused `diarization` Rust module for pure alignment/status logic, SQLite fields for resolved speaker labels, Tauri commands for settings/status, and frontend types/settings/rendering that can consume live or offline diarization results later. FluidAudio/Swift sidecar integration is intentionally a follow-up plan after this foundation is merged.

**Tech Stack:** Tauri 2, Rust, sqlx SQLite migrations, Next.js/React, TypeScript, existing shadcn/Radix UI primitives, localStorage beta flags.

---

## Working directory and baseline

Implementation worktree:

```text
/Users/themaddoxed/Desktop/meeting-minutes/.worktrees/speaker-diarization
```

Branch:

```text
feature/speaker-diarization
```

Baseline status observed before implementation:

- `cargo test --workspace --no-run` fetches dependencies with network access, then fails before feature code because `cidre` requires full Xcode:

```text
xcode-select: error: tool 'xcodebuild' requires Xcode, but active developer directory '/Library/Developer/CommandLineTools' is a command line tools instance
```

- Frontend baseline was not run because this clean worktree has no `frontend/node_modules` and no committed npm/pnpm/yarn lockfile. Do not run `npm install` casually because it may create a new lockfile unrelated to this feature.

Before claiming full verification, either:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

or use an environment where the Rust workspace already builds with the macOS private framework dependencies.

## Scope of this first feature slice

This plan implements:

- speaker diarization domain types;
- deterministic diarization interval to transcript alignment;
- SQLite schema for resolved transcript speaker metadata and raw speaker intervals;
- persisted diarization settings;
- beta flag and settings UI;
- transcript display of speaker labels, provisional/final/overlap states;
- copy transcript with speaker labels;
- placeholder-safe Tauri commands that allow later live/offline engines to write diarization results.

This plan does not implement:

- FluidAudio Swift sidecar;
- online clustering;
- offline VBx processing;
- separate audio-stem recording changes;
- pyannote enhancement.

Those belong in the next plan because they touch capture, packaging, model download, sidecar lifecycle, and macOS build setup.

## File structure

Create:

- `frontend/src-tauri/src/diarization/mod.rs` — module exports.
- `frontend/src-tauri/src/diarization/types.rs` — serializable diarization domain types.
- `frontend/src-tauri/src/diarization/alignment.rs` — pure interval-to-transcript alignment.
- `frontend/src-tauri/src/diarization/commands.rs` — Tauri commands for settings/status and applying segment assignments.
- `frontend/src-tauri/src/database/repositories/diarization.rs` — SQLite access for diarization settings, meeting status, and speaker segments.
- `frontend/src-tauri/migrations/20260626000100_add_diarization_foundation.sql` — additive migration.
- `frontend/src/types/diarization.ts` — frontend diarization types.
- `frontend/src/components/settings/SettingsSection.tsx` — generic settings card primitive.
- `frontend/src/components/settings/SettingsRow.tsx` — generic settings row primitive.
- `frontend/src/components/settings/SettingsStatusBadge.tsx` — reusable status badge.
- `frontend/src/components/SpeakerDiarizationSettings.tsx` — feature settings block.

Modify:

- `frontend/src-tauri/src/lib.rs` — register module and commands.
- `frontend/src-tauri/src/database/repositories/mod.rs` if present; otherwise update imports at call sites.
- `frontend/src-tauri/src/database/models.rs` — add diarization fields to `Transcript`; add setting/status structs.
- `frontend/src-tauri/src/database/repositories/meeting.rs` — map diarization fields into API response.
- `frontend/src-tauri/src/database/repositories/transcript.rs` — save default diarization metadata.
- `frontend/src-tauri/src/api/api.rs` — add diarization fields to `MeetingTranscript` and `TranscriptSegment`.
- `frontend/src-tauri/src/audio/transcription/worker.rs` — emit diarization-neutral fields only if type changes are required.
- `frontend/src/types/index.ts` — add speaker fields to transcript types.
- `frontend/src/services/configService.ts` — add diarization settings API wrappers.
- `frontend/src/contexts/ConfigContext.tsx` — store diarization settings and actions.
- `frontend/src/types/betaFeatures.ts` — add `speakerDiarization`.
- `frontend/src/components/BetaSettings.tsx` — render the beta toggle.
- `frontend/src/components/TranscriptSettings.tsx` — render `SpeakerDiarizationSettings` when beta flag is enabled.
- `frontend/src/components/VirtualizedTranscriptView.tsx` — render speaker labels and badges.
- `frontend/src/app/_components/TranscriptPanel.tsx` — pass speaker fields into virtualized segments.
- `frontend/src/contexts/TranscriptContext.tsx` — preserve speaker fields in live updates and copy output.
- `frontend/src/services/indexedDBService.ts` — tolerate additional diarization fields in stored transcript updates.

## Task 1: Rust diarization domain types

**Files:**

- Create: `frontend/src-tauri/src/diarization/mod.rs`
- Create: `frontend/src-tauri/src/diarization/types.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Write the failing compile-oriented unit test**

Create `frontend/src-tauri/src/diarization/types.rs` with the tests first:

```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diarization_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&DiarizationStatus::FallbackToLive).unwrap();
        assert_eq!(json, "\"fallback_to_live\"");
    }

    #[test]
    fn overlap_assignment_uses_multiple_speakers_label() {
        let assignment = SpeakerAssignment::overlap(0.91, DiarizationStatus::Provisional, "fluid_audio_online");

        assert_eq!(assignment.speaker_id, None);
        assert_eq!(assignment.speaker_label.as_deref(), Some("Multiple speakers"));
        assert!(assignment.is_overlap);
        assert_eq!(assignment.diarization_status, DiarizationStatus::Provisional);
        assert_eq!(assignment.diarization_method.as_deref(), Some("fluid_audio_online"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p meetily diarization::types::tests::diarization_status_serializes_as_snake_case
```

Expected before implementation: compile fails because `DiarizationStatus` and `SpeakerAssignment` are not defined.

- [ ] **Step 3: Implement the minimal types**

Replace `frontend/src-tauri/src/diarization/types.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiarizationStatus {
    None,
    Provisional,
    Final,
    FallbackToLive,
    Failed,
    NeedsReview,
}

impl Default for DiarizationStatus {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub meeting_id: String,
    pub source: String,
    pub start_time: f64,
    pub end_time: f64,
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub confidence: Option<f64>,
    pub is_overlap: bool,
    pub diarization_status: DiarizationStatus,
    pub diarization_method: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWindow {
    pub transcript_id: String,
    pub audio_start_time: Option<f64>,
    pub audio_end_time: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerAssignment {
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub speaker_color: Option<String>,
    pub is_overlap: bool,
    pub diarization_status: DiarizationStatus,
    pub diarization_method: Option<String>,
    pub diarization_confidence: Option<f64>,
}

impl SpeakerAssignment {
    pub fn unknown(status: DiarizationStatus, method: impl Into<Option<String>>) -> Self {
        Self {
            speaker_id: None,
            speaker_label: Some("Unknown speaker".to_string()),
            speaker_color: None,
            is_overlap: false,
            diarization_status: status,
            diarization_method: method.into(),
            diarization_confidence: None,
        }
    }

    pub fn overlap(confidence: f64, status: DiarizationStatus, method: impl Into<String>) -> Self {
        Self {
            speaker_id: None,
            speaker_label: Some("Multiple speakers".to_string()),
            speaker_color: Some("#f97316".to_string()),
            is_overlap: true,
            diarization_status: status,
            diarization_method: Some(method.into()),
            diarization_confidence: Some(confidence),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diarization_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&DiarizationStatus::FallbackToLive).unwrap();
        assert_eq!(json, "\"fallback_to_live\"");
    }

    #[test]
    fn overlap_assignment_uses_multiple_speakers_label() {
        let assignment = SpeakerAssignment::overlap(0.91, DiarizationStatus::Provisional, "fluid_audio_online");

        assert_eq!(assignment.speaker_id, None);
        assert_eq!(assignment.speaker_label.as_deref(), Some("Multiple speakers"));
        assert!(assignment.is_overlap);
        assert_eq!(assignment.diarization_status, DiarizationStatus::Provisional);
        assert_eq!(assignment.diarization_method.as_deref(), Some("fluid_audio_online"));
    }
}
```

Create `frontend/src-tauri/src/diarization/mod.rs`:

```rust
pub mod alignment;
pub mod commands;
pub mod types;
```

Add this module declaration near the other module declarations in `frontend/src-tauri/src/lib.rs`:

```rust
pub mod diarization;
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test -p meetily diarization::types::tests::diarization_status_serializes_as_snake_case
```

Expected after implementation: PASS when the Xcode baseline blocker is resolved.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization frontend/src-tauri/src/lib.rs
git commit -m "feat: add diarization domain types"
```

## Task 2: Deterministic transcript alignment

**Files:**

- Modify: `frontend/src-tauri/src/diarization/alignment.rs`
- Modify: `frontend/src-tauri/src/diarization/mod.rs`

- [ ] **Step 1: Write failing alignment tests**

Create `frontend/src-tauri/src/diarization/alignment.rs`:

```rust
use super::types::{DiarizationStatus, SpeakerAssignment, SpeakerSegment, TranscriptWindow};

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(label: &str, start: f64, end: f64) -> SpeakerSegment {
        SpeakerSegment {
            meeting_id: "meeting-1".to_string(),
            source: "system".to_string(),
            start_time: start,
            end_time: end,
            speaker_id: Some(label.to_lowercase().replace(' ', "-")),
            speaker_label: Some(label.to_string()),
            confidence: Some(0.8),
            is_overlap: false,
            diarization_status: DiarizationStatus::Provisional,
            diarization_method: Some("unit_test".to_string()),
        }
    }

    #[test]
    fn assigns_speaker_with_largest_temporal_overlap() {
        let transcript = TranscriptWindow {
            transcript_id: "t1".to_string(),
            audio_start_time: Some(10.0),
            audio_end_time: Some(16.0),
        };
        let segments = vec![
            segment("Speaker 1", 10.0, 12.0),
            segment("Speaker 2", 12.0, 16.0),
        ];

        let assignment = assign_speaker_to_transcript(&transcript, &segments, 0.1, "unit_test");

        assert_eq!(assignment.speaker_label.as_deref(), Some("Speaker 2"));
        assert_eq!(assignment.speaker_id.as_deref(), Some("speaker-2"));
        assert!(!assignment.is_overlap);
    }

    #[test]
    fn marks_overlap_when_matching_segment_is_overlap() {
        let transcript = TranscriptWindow {
            transcript_id: "t2".to_string(),
            audio_start_time: Some(20.0),
            audio_end_time: Some(24.0),
        };
        let mut overlap = segment("Speaker 1", 20.0, 24.0);
        overlap.is_overlap = true;
        overlap.confidence = Some(0.93);

        let assignment = assign_speaker_to_transcript(&transcript, &[overlap], 0.1, "unit_test");

        assert_eq!(assignment.speaker_label.as_deref(), Some("Multiple speakers"));
        assert!(assignment.is_overlap);
        assert_eq!(assignment.diarization_confidence, Some(0.93));
    }

    #[test]
    fn returns_unknown_when_transcript_has_no_audio_window() {
        let transcript = TranscriptWindow {
            transcript_id: "t3".to_string(),
            audio_start_time: None,
            audio_end_time: None,
        };

        let assignment = assign_speaker_to_transcript(&transcript, &[segment("Speaker 1", 0.0, 1.0)], 0.1, "unit_test");

        assert_eq!(assignment.speaker_label.as_deref(), Some("Unknown speaker"));
        assert_eq!(assignment.diarization_status, DiarizationStatus::NeedsReview);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p meetily diarization::alignment::tests
```

Expected before implementation: compile fails because `assign_speaker_to_transcript` is missing.

- [ ] **Step 3: Implement minimal alignment**

Insert above the test module in `frontend/src-tauri/src/diarization/alignment.rs`:

```rust
pub fn assign_speaker_to_transcript(
    transcript: &TranscriptWindow,
    segments: &[SpeakerSegment],
    min_overlap_ratio: f64,
    method: &str,
) -> SpeakerAssignment {
    let Some(transcript_start) = transcript.audio_start_time else {
        return SpeakerAssignment::unknown(DiarizationStatus::NeedsReview, Some(method.to_string()));
    };
    let Some(transcript_end) = transcript.audio_end_time else {
        return SpeakerAssignment::unknown(DiarizationStatus::NeedsReview, Some(method.to_string()));
    };

    let transcript_duration = (transcript_end - transcript_start).max(0.0);
    if transcript_duration == 0.0 {
        return SpeakerAssignment::unknown(DiarizationStatus::NeedsReview, Some(method.to_string()));
    }

    let mut best_segment: Option<(&SpeakerSegment, f64)> = None;

    for segment in segments {
        let overlap_start = transcript_start.max(segment.start_time);
        let overlap_end = transcript_end.min(segment.end_time);
        let overlap = (overlap_end - overlap_start).max(0.0);
        if overlap <= 0.0 {
            continue;
        }

        let ratio = overlap / transcript_duration;
        if ratio < min_overlap_ratio {
            continue;
        }

        match best_segment {
            Some((_, best_overlap)) if best_overlap >= overlap => {}
            _ => best_segment = Some((segment, overlap)),
        }
    }

    let Some((segment, _)) = best_segment else {
        return SpeakerAssignment::unknown(DiarizationStatus::NeedsReview, Some(method.to_string()));
    };

    if segment.is_overlap {
        return SpeakerAssignment::overlap(
            segment.confidence.unwrap_or(0.0),
            segment.diarization_status,
            method.to_string(),
        );
    }

    SpeakerAssignment {
        speaker_id: segment.speaker_id.clone(),
        speaker_label: segment.speaker_label.clone().or_else(|| Some("Unknown speaker".to_string())),
        speaker_color: None,
        is_overlap: false,
        diarization_status: segment.diarization_status,
        diarization_method: Some(method.to_string()),
        diarization_confidence: segment.confidence,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p meetily diarization::alignment::tests
```

Expected after implementation: PASS when baseline build environment is fixed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/diarization/alignment.rs
git commit -m "feat: align diarization segments to transcripts"
```

## Task 3: SQLite schema and Rust database models

**Files:**

- Create: `frontend/src-tauri/migrations/20260626000100_add_diarization_foundation.sql`
- Modify: `frontend/src-tauri/src/database/models.rs`
- Modify: `frontend/src-tauri/src/api/api.rs`

- [ ] **Step 1: Write schema migration**

Create `frontend/src-tauri/migrations/20260626000100_add_diarization_foundation.sql`:

```sql
-- Speaker diarization foundation.
-- Existing transcripts.speaker is legacy source metadata (mic/system) and remains untouched.

ALTER TABLE transcripts ADD COLUMN speaker_id TEXT;
ALTER TABLE transcripts ADD COLUMN speaker_label TEXT;
ALTER TABLE transcripts ADD COLUMN speaker_color TEXT;
ALTER TABLE transcripts ADD COLUMN is_overlap INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcripts ADD COLUMN diarization_status TEXT NOT NULL DEFAULT 'none';
ALTER TABLE transcripts ADD COLUMN diarization_method TEXT;
ALTER TABLE transcripts ADD COLUMN diarization_confidence REAL;

CREATE TABLE IF NOT EXISTS diarization_settings (
    id TEXT PRIMARY KEY NOT NULL DEFAULT '1',
    enabled INTEGER NOT NULL DEFAULT 0,
    mode TEXT NOT NULL DEFAULT 'live_plus_post_call',
    show_provisional_labels INTEGER NOT NULL DEFAULT 1,
    post_call_refinement_enabled INTEGER NOT NULL DEFAULT 1,
    overlap_handling TEXT NOT NULL DEFAULT 'multiple_speakers',
    speaker_review_enabled INTEGER NOT NULL DEFAULT 1,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO diarization_settings (id)
VALUES ('1')
ON CONFLICT(id) DO NOTHING;

CREATE TABLE IF NOT EXISTS meeting_diarization_status (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    live_enabled INTEGER NOT NULL DEFAULT 0,
    post_call_enabled INTEGER NOT NULL DEFAULT 0,
    current_status TEXT NOT NULL DEFAULT 'none',
    quality_flags TEXT,
    processed_at DATETIME,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS speaker_segments (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL,
    source TEXT NOT NULL,
    start_time REAL NOT NULL,
    end_time REAL NOT NULL,
    speaker_id TEXT,
    speaker_label TEXT,
    confidence REAL,
    is_overlap INTEGER NOT NULL DEFAULT 0,
    diarization_status TEXT NOT NULL,
    diarization_method TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_speaker_segments_meeting_time
ON speaker_segments(meeting_id, start_time, end_time);

CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_diarization
ON transcripts(meeting_id, diarization_status);
```

- [ ] **Step 2: Update Rust models**

In `frontend/src-tauri/src/database/models.rs`, add fields to `Transcript` after `duration`:

```rust
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub speaker_color: Option<String>,
    pub is_overlap: Option<i64>,
    pub diarization_status: Option<String>,
    pub diarization_method: Option<String>,
    pub diarization_confidence: Option<f64>,
```

Add these structs after `TranscriptSetting`:

```rust
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DiarizationSetting {
    pub id: String,
    pub enabled: i64,
    pub mode: String,
    pub show_provisional_labels: i64,
    pub post_call_refinement_enabled: i64,
    pub overlap_handling: String,
    pub speaker_review_enabled: i64,
    pub updated_at: DateTimeUtc,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MeetingDiarizationStatus {
    pub meeting_id: String,
    pub enabled: i64,
    pub live_enabled: i64,
    pub post_call_enabled: i64,
    pub current_status: String,
    pub quality_flags: Option<String>,
    pub processed_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SpeakerSegmentModel {
    pub id: String,
    pub meeting_id: String,
    pub source: String,
    pub start_time: f64,
    pub end_time: f64,
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub confidence: Option<f64>,
    pub is_overlap: i64,
    pub diarization_status: String,
    pub diarization_method: Option<String>,
    pub created_at: DateTimeUtc,
}
```

- [ ] **Step 3: Update API transcript DTOs**

In `frontend/src-tauri/src/api/api.rs`, add to `MeetingTranscript`:

```rust
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub speaker_color: Option<String>,
    pub is_overlap: Option<bool>,
    pub diarization_status: Option<String>,
    pub diarization_method: Option<String>,
    pub diarization_confidence: Option<f64>,
```

Add equivalent optional fields to `TranscriptSegment`:

```rust
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub speaker_color: Option<String>,
    pub is_overlap: Option<bool>,
    pub diarization_status: Option<String>,
    pub diarization_method: Option<String>,
    pub diarization_confidence: Option<f64>,
```

- [ ] **Step 4: Run compile check**

Run:

```bash
cargo check -p meetily
```

Expected before fixing mapping code: compile errors in repository mappings that construct `MeetingTranscript` or save `TranscriptSegment`.

- [ ] **Step 5: Commit migration and model changes after Task 4 fixes mapping**

Do not commit this task alone if compile is broken. Commit together with Task 4 after all DTO construction sites are updated.

## Task 4: Repository mapping and save defaults

**Files:**

- Modify: `frontend/src-tauri/src/database/repositories/meeting.rs`
- Modify: `frontend/src-tauri/src/database/repositories/transcript.rs`

- [ ] **Step 1: Update meeting transcript mapping**

In `frontend/src-tauri/src/database/repositories/meeting.rs`, update the `MeetingTranscript` mapping:

```rust
MeetingTranscript {
    id: t.id,
    text: t.transcript,
    timestamp: t.timestamp,
    audio_start_time: t.audio_start_time,
    audio_end_time: t.audio_end_time,
    duration: t.duration,
    speaker_id: t.speaker_id,
    speaker_label: t.speaker_label,
    speaker_color: t.speaker_color,
    is_overlap: t.is_overlap.map(|value| value != 0),
    diarization_status: t.diarization_status,
    diarization_method: t.diarization_method,
    diarization_confidence: t.diarization_confidence,
}
```

- [ ] **Step 2: Update transcript save insert**

In `frontend/src-tauri/src/database/repositories/transcript.rs`, replace the insert query inside `save_transcript` with:

```rust
let result = sqlx::query(
    "INSERT INTO transcripts (
        id, meeting_id, transcript, timestamp,
        audio_start_time, audio_end_time, duration,
        speaker_id, speaker_label, speaker_color, is_overlap,
        diarization_status, diarization_method, diarization_confidence
     )
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
)
.bind(&transcript_id)
.bind(&meeting_id)
.bind(&segment.text)
.bind(&segment.timestamp)
.bind(segment.audio_start_time)
.bind(segment.audio_end_time)
.bind(segment.duration)
.bind(&segment.speaker_id)
.bind(&segment.speaker_label)
.bind(&segment.speaker_color)
.bind(segment.is_overlap.unwrap_or(false) as i64)
.bind(segment.diarization_status.as_deref().unwrap_or("none"))
.bind(&segment.diarization_method)
.bind(segment.diarization_confidence)
.execute(&mut *transaction)
.await;
```

- [ ] **Step 3: Run compile check**

Run:

```bash
cargo check -p meetily
```

Expected after mapping fixes: no diarization-related compile errors. Environment may still fail on the Xcode/cidre blocker until Xcode is selected.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/migrations/20260626000100_add_diarization_foundation.sql frontend/src-tauri/src/database/models.rs frontend/src-tauri/src/api/api.rs frontend/src-tauri/src/database/repositories/meeting.rs frontend/src-tauri/src/database/repositories/transcript.rs
git commit -m "feat: persist transcript diarization metadata"
```

## Task 5: Diarization settings repository and commands

**Files:**

- Create: `frontend/src-tauri/src/database/repositories/diarization.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs` if it exists
- Modify: `frontend/src-tauri/src/diarization/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Create repository**

Create `frontend/src-tauri/src/database/repositories/diarization.rs`:

```rust
use crate::database::models::DiarizationSetting;
use sqlx::SqlitePool;

pub struct DiarizationRepository;

impl DiarizationRepository {
    pub async fn get_settings(pool: &SqlitePool) -> Result<DiarizationSetting, sqlx::Error> {
        let existing = sqlx::query_as::<_, DiarizationSetting>(
            "SELECT * FROM diarization_settings WHERE id = '1'"
        )
        .fetch_optional(pool)
        .await?;

        if let Some(settings) = existing {
            return Ok(settings);
        }

        sqlx::query("INSERT INTO diarization_settings (id) VALUES ('1')")
            .execute(pool)
            .await?;

        sqlx::query_as::<_, DiarizationSetting>(
            "SELECT * FROM diarization_settings WHERE id = '1'"
        )
        .fetch_one(pool)
        .await
    }

    pub async fn save_settings(
        pool: &SqlitePool,
        enabled: bool,
        mode: &str,
        show_provisional_labels: bool,
        post_call_refinement_enabled: bool,
        overlap_handling: &str,
        speaker_review_enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO diarization_settings (
                id, enabled, mode, show_provisional_labels,
                post_call_refinement_enabled, overlap_handling,
                speaker_review_enabled, updated_at
             )
             VALUES ('1', ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                mode = excluded.mode,
                show_provisional_labels = excluded.show_provisional_labels,
                post_call_refinement_enabled = excluded.post_call_refinement_enabled,
                overlap_handling = excluded.overlap_handling,
                speaker_review_enabled = excluded.speaker_review_enabled,
                updated_at = CURRENT_TIMESTAMP"
        )
        .bind(enabled as i64)
        .bind(mode)
        .bind(show_provisional_labels as i64)
        .bind(post_call_refinement_enabled as i64)
        .bind(overlap_handling)
        .bind(speaker_review_enabled as i64)
        .execute(pool)
        .await?;

        Ok(())
    }
}
```

If `frontend/src-tauri/src/database/repositories/mod.rs` exists, add:

```rust
pub mod diarization;
```

- [ ] **Step 2: Create Tauri command DTOs and commands**

Create `frontend/src-tauri/src/diarization/commands.rs`:

```rust
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State};

use crate::{
    database::repositories::diarization::DiarizationRepository,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationSettingsDto {
    pub enabled: bool,
    pub mode: String,
    pub show_provisional_labels: bool,
    pub post_call_refinement_enabled: bool,
    pub overlap_handling: String,
    pub speaker_review_enabled: bool,
}

#[tauri::command]
pub async fn get_diarization_settings<R: Runtime>(
    state: State<'_, AppState>,
) -> Result<DiarizationSettingsDto, String> {
    let settings = DiarizationRepository::get_settings(state.db_manager.pool())
        .await
        .map_err(|error| error.to_string())?;

    Ok(DiarizationSettingsDto {
        enabled: settings.enabled != 0,
        mode: settings.mode,
        show_provisional_labels: settings.show_provisional_labels != 0,
        post_call_refinement_enabled: settings.post_call_refinement_enabled != 0,
        overlap_handling: settings.overlap_handling,
        speaker_review_enabled: settings.speaker_review_enabled != 0,
    })
}

#[tauri::command]
pub async fn save_diarization_settings<R: Runtime>(
    state: State<'_, AppState>,
    settings: DiarizationSettingsDto,
) -> Result<(), String> {
    DiarizationRepository::save_settings(
        state.db_manager.pool(),
        settings.enabled,
        &settings.mode,
        settings.show_provisional_labels,
        settings.post_call_refinement_enabled,
        &settings.overlap_handling,
        settings.speaker_review_enabled,
    )
    .await
    .map_err(|error| error.to_string())
}
```

- [ ] **Step 3: Register commands**

In `frontend/src-tauri/src/lib.rs`, add command registrations in the existing `tauri::generate_handler!` list:

```rust
diarization::commands::get_diarization_settings,
diarization::commands::save_diarization_settings,
```

- [ ] **Step 4: Run compile check**

Run:

```bash
cargo check -p meetily
```

Expected: command registration compiles when environment baseline is fixed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/diarization.rs frontend/src-tauri/src/database/repositories/mod.rs frontend/src-tauri/src/diarization/commands.rs frontend/src-tauri/src/lib.rs
git commit -m "feat: add diarization settings commands"
```

If `frontend/src-tauri/src/database/repositories/mod.rs` does not exist, omit it from `git add`.

## Task 6: Frontend diarization settings state and beta flag

**Files:**

- Create: `frontend/src/types/diarization.ts`
- Modify: `frontend/src/types/betaFeatures.ts`
- Modify: `frontend/src/services/configService.ts`
- Modify: `frontend/src/contexts/ConfigContext.tsx`
- Modify: `frontend/src/components/BetaSettings.tsx`

- [ ] **Step 1: Add frontend type file**

Create `frontend/src/types/diarization.ts`:

```typescript
export type DiarizationMode = 'live_plus_post_call' | 'post_call_only' | 'off';
export type OverlapHandling = 'multiple_speakers';
export type DiarizationStatus =
  | 'none'
  | 'provisional'
  | 'final'
  | 'fallback_to_live'
  | 'failed'
  | 'needs_review';

export interface DiarizationSettings {
  enabled: boolean;
  mode: DiarizationMode;
  showProvisionalLabels: boolean;
  postCallRefinementEnabled: boolean;
  overlapHandling: OverlapHandling;
  speakerReviewEnabled: boolean;
}

export const DEFAULT_DIARIZATION_SETTINGS: DiarizationSettings = {
  enabled: false,
  mode: 'live_plus_post_call',
  showProvisionalLabels: true,
  postCallRefinementEnabled: true,
  overlapHandling: 'multiple_speakers',
  speakerReviewEnabled: true,
};
```

- [ ] **Step 2: Add beta flag**

In `frontend/src/types/betaFeatures.ts`, update `BetaFeatures`:

```typescript
  speakerDiarization: boolean;
```

Update `DEFAULT_BETA_FEATURES`:

```typescript
  speakerDiarization: false,
```

Update `BETA_FEATURE_NAMES`:

```typescript
  speakerDiarization: 'Speaker Labels',
```

Update `BETA_FEATURE_DESCRIPTIONS`:

```typescript
  speakerDiarization: 'Label speakers in transcripts with live provisional labels and post-call refinement.',
```

- [ ] **Step 3: Render beta flag**

In `frontend/src/components/BetaSettings.tsx`, change:

```typescript
const featureOrder: BetaFeatureKey[] = ['importAndRetranscribe'];
```

to:

```typescript
const featureOrder: BetaFeatureKey[] = ['importAndRetranscribe', 'speakerDiarization'];
```

- [ ] **Step 4: Add config service wrappers**

In `frontend/src/services/configService.ts`, import:

```typescript
import { DiarizationSettings } from '@/types/diarization';
```

Add methods inside `ConfigService`:

```typescript
async getDiarizationSettings(): Promise<DiarizationSettings> {
  return invoke<DiarizationSettings>('get_diarization_settings');
}

async saveDiarizationSettings(settings: DiarizationSettings): Promise<void> {
  return invoke<void>('save_diarization_settings', { settings });
}
```

- [ ] **Step 5: Add context state and actions**

In `frontend/src/contexts/ConfigContext.tsx`, import:

```typescript
import { DEFAULT_DIARIZATION_SETTINGS, DiarizationSettings } from '@/types/diarization';
```

Add to `ConfigContextType`:

```typescript
  diarizationSettings: DiarizationSettings;
  isLoadingDiarizationSettings: boolean;
  updateDiarizationSettings: (settings: DiarizationSettings) => Promise<void>;
```

Add state in `ConfigProvider`:

```typescript
const [diarizationSettings, setDiarizationSettings] = useState<DiarizationSettings>(DEFAULT_DIARIZATION_SETTINGS);
const [isLoadingDiarizationSettings, setIsLoadingDiarizationSettings] = useState(false);
```

Add load effect:

```typescript
useEffect(() => {
  const loadDiarizationSettings = async () => {
    setIsLoadingDiarizationSettings(true);
    try {
      const settings = await configService.getDiarizationSettings();
      setDiarizationSettings(settings);
    } catch (error) {
      console.error('[ConfigContext] Failed to load diarization settings:', error);
      setDiarizationSettings(DEFAULT_DIARIZATION_SETTINGS);
    } finally {
      setIsLoadingDiarizationSettings(false);
    }
  };

  loadDiarizationSettings();
}, []);
```

Add action:

```typescript
const updateDiarizationSettings = useCallback(async (settings: DiarizationSettings) => {
  setDiarizationSettings(settings);
  await configService.saveDiarizationSettings(settings);
}, []);
```

Add these fields to the `value` object:

```typescript
diarizationSettings,
isLoadingDiarizationSettings,
updateDiarizationSettings,
```

- [ ] **Step 6: Run TypeScript verification**

Run only after dependencies are installed intentionally:

```bash
cd frontend
npm run build
```

Expected: TypeScript compiles after command names and context fields are consistent.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/types/diarization.ts frontend/src/types/betaFeatures.ts frontend/src/services/configService.ts frontend/src/contexts/ConfigContext.tsx frontend/src/components/BetaSettings.tsx
git commit -m "feat: add diarization settings state"
```

## Task 7: Settings primitives and speaker diarization settings UI

**Files:**

- Create: `frontend/src/components/settings/SettingsSection.tsx`
- Create: `frontend/src/components/settings/SettingsRow.tsx`
- Create: `frontend/src/components/settings/SettingsStatusBadge.tsx`
- Create: `frontend/src/components/SpeakerDiarizationSettings.tsx`
- Modify: `frontend/src/components/TranscriptSettings.tsx`

- [ ] **Step 1: Add generic settings primitives**

Create `frontend/src/components/settings/SettingsSection.tsx`:

```tsx
import React from 'react';

interface SettingsSectionProps {
  title: string;
  description?: string;
  badge?: React.ReactNode;
  children: React.ReactNode;
}

export function SettingsSection({ title, description, badge, children }: SettingsSectionProps) {
  return (
    <section className="bg-card text-card-foreground rounded-lg border border-border p-6 shadow-sm">
      <div className="mb-5">
        <div className="flex items-center gap-2">
          <h3 className="text-lg font-semibold text-foreground">{title}</h3>
          {badge}
        </div>
        {description && (
          <p className="mt-2 text-sm text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="space-y-3">{children}</div>
    </section>
  );
}
```

Create `frontend/src/components/settings/SettingsRow.tsx`:

```tsx
import React from 'react';

interface SettingsRowProps {
  title: string;
  description?: string;
  control: React.ReactNode;
  disabledReason?: string;
}

export function SettingsRow({ title, description, control, disabledReason }: SettingsRowProps) {
  return (
    <div className="flex items-center justify-between gap-6 rounded-lg border border-border p-4">
      <div className="min-w-0 flex-1">
        <div className="font-medium text-foreground">{title}</div>
        {description && (
          <div className="mt-1 text-sm text-muted-foreground">{description}</div>
        )}
        {disabledReason && (
          <div className="mt-2 text-xs text-amber-700">{disabledReason}</div>
        )}
      </div>
      <div className="flex-shrink-0">{control}</div>
    </div>
  );
}
```

Create `frontend/src/components/settings/SettingsStatusBadge.tsx`:

```tsx
import React from 'react';
import { cn } from '@/lib/utils';

type SettingsStatusBadgeTone = 'beta' | 'local' | 'provisional' | 'final' | 'fallback' | 'review' | 'unavailable';

const toneClassName: Record<SettingsStatusBadgeTone, string> = {
  beta: 'bg-yellow-100 text-yellow-800',
  local: 'bg-blue-100 text-blue-800',
  provisional: 'bg-amber-100 text-amber-800',
  final: 'bg-green-100 text-green-800',
  fallback: 'bg-orange-100 text-orange-800',
  review: 'bg-purple-100 text-purple-800',
  unavailable: 'bg-gray-100 text-gray-700',
};

interface SettingsStatusBadgeProps {
  tone: SettingsStatusBadgeTone;
  children: React.ReactNode;
}

export function SettingsStatusBadge({ tone, children }: SettingsStatusBadgeProps) {
  return (
    <span className={cn('inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium', toneClassName[tone])}>
      {children}
    </span>
  );
}
```

- [ ] **Step 2: Add feature settings component**

Create `frontend/src/components/SpeakerDiarizationSettings.tsx`:

```tsx
'use client';

import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { SettingsSection } from '@/components/settings/SettingsSection';
import { SettingsRow } from '@/components/settings/SettingsRow';
import { SettingsStatusBadge } from '@/components/settings/SettingsStatusBadge';
import { useConfig } from '@/contexts/ConfigContext';
import { DiarizationMode } from '@/types/diarization';

export function SpeakerDiarizationSettings() {
  const {
    betaFeatures,
    diarizationSettings,
    isLoadingDiarizationSettings,
    updateDiarizationSettings,
  } = useConfig();

  if (!betaFeatures.speakerDiarization) {
    return null;
  }

  const update = (patch: Partial<typeof diarizationSettings>) => {
    updateDiarizationSettings({ ...diarizationSettings, ...patch })
      .catch(error => console.error('[SpeakerDiarizationSettings] Failed to save:', error));
  };

  const liveModeEnabled = diarizationSettings.enabled && diarizationSettings.mode === 'live_plus_post_call';

  return (
    <SettingsSection
      title="Speaker labels"
      description="Label who spoke in the transcript. Live labels are provisional; final labels are applied after the meeting."
      badge={<SettingsStatusBadge tone="beta">Beta</SettingsStatusBadge>}
    >
      <SettingsRow
        title="Enable speaker diarization"
        description="Add speaker labels to transcripts without identifying people by voice."
        control={
          <Switch
            checked={diarizationSettings.enabled}
            disabled={isLoadingDiarizationSettings}
            onCheckedChange={(enabled) => update({ enabled, mode: enabled ? diarizationSettings.mode : 'off' })}
          />
        }
      />

      <SettingsRow
        title="Mode"
        description="Recommended: show provisional labels during the call, then refine after recording stops."
        control={
          <Select
            value={diarizationSettings.mode}
            disabled={!diarizationSettings.enabled || isLoadingDiarizationSettings}
            onValueChange={(mode) => update({ mode: mode as DiarizationMode })}
          >
            <SelectTrigger className="w-[260px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="live_plus_post_call">Live + post-call refinement</SelectItem>
              <SelectItem value="post_call_only">Post-call only</SelectItem>
              <SelectItem value="off">Off</SelectItem>
            </SelectContent>
          </Select>
        }
      />

      <SettingsRow
        title="Show provisional labels during call"
        description="Display live Speaker 1 / Speaker 2 labels with a provisional badge."
        disabledReason={!liveModeEnabled ? 'Available only in Live + post-call refinement mode.' : undefined}
        control={
          <Switch
            checked={diarizationSettings.showProvisionalLabels}
            disabled={!liveModeEnabled || isLoadingDiarizationSettings}
            onCheckedChange={(showProvisionalLabels) => update({ showProvisionalLabels })}
          />
        }
      />

      <SettingsRow
        title="Run post-call refinement automatically"
        description="After the meeting, analyze the full audio and replace provisional labels when quality is good."
        control={
          <Switch
            checked={diarizationSettings.postCallRefinementEnabled}
            disabled={!diarizationSettings.enabled || diarizationSettings.mode === 'off' || isLoadingDiarizationSettings}
            onCheckedChange={(postCallRefinementEnabled) => update({ postCallRefinementEnabled })}
          />
        }
      />

      <SettingsRow
        title="Overlap handling"
        description="Interruptions are marked as Multiple speakers."
        control={<SettingsStatusBadge tone="local">Multiple speakers</SettingsStatusBadge>}
      />

      <SettingsRow
        title="Speaker review"
        description="Allow rename, merge, split, and overlap fixes after post-call processing."
        control={
          <Switch
            checked={diarizationSettings.speakerReviewEnabled}
            disabled={!diarizationSettings.enabled || isLoadingDiarizationSettings}
            onCheckedChange={(speakerReviewEnabled) => update({ speakerReviewEnabled })}
          />
        }
      />
    </SettingsSection>
  );
}
```

- [ ] **Step 3: Render settings inside Transcript settings**

In `frontend/src/components/TranscriptSettings.tsx`, import:

```tsx
import { SpeakerDiarizationSettings } from './SpeakerDiarizationSettings';
```

Render at the end of the outer `space-y-4 pb-6` container:

```tsx
<SpeakerDiarizationSettings />
```

- [ ] **Step 4: Run frontend verification**

Run after dependencies are intentionally installed:

```bash
cd frontend
npm run build
```

Expected: TypeScript and Next build pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/settings/SettingsSection.tsx frontend/src/components/settings/SettingsRow.tsx frontend/src/components/settings/SettingsStatusBadge.tsx frontend/src/components/SpeakerDiarizationSettings.tsx frontend/src/components/TranscriptSettings.tsx
git commit -m "feat: add speaker diarization settings UI"
```

## Task 8: Frontend transcript speaker rendering

**Files:**

- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/app/_components/TranscriptPanel.tsx`
- Modify: `frontend/src/components/VirtualizedTranscriptView.tsx`
- Modify: `frontend/src/contexts/TranscriptContext.tsx`

- [ ] **Step 1: Extend frontend transcript types**

In `frontend/src/types/index.ts`, add to `Transcript`, `TranscriptUpdate`, and `TranscriptSegmentData`:

```typescript
speaker_id?: string | null;
speaker_label?: string | null;
speaker_color?: string | null;
is_overlap?: boolean | null;
diarization_status?: 'none' | 'provisional' | 'final' | 'fallback_to_live' | 'failed' | 'needs_review' | null;
diarization_method?: string | null;
diarization_confidence?: number | null;
```

- [ ] **Step 2: Preserve fields in transcript context**

In `frontend/src/contexts/TranscriptContext.tsx`, when constructing `newTranscript` from `update`, add:

```typescript
speaker_id: update.speaker_id,
speaker_label: update.speaker_label,
speaker_color: update.speaker_color,
is_overlap: update.is_overlap,
diarization_status: update.diarization_status,
diarization_method: update.diarization_method,
diarization_confidence: update.diarization_confidence,
```

Apply this in:

- main `transcriptService.onTranscriptUpdate` listener;
- reload sync `formattedTranscripts`;
- manual `addTranscript`.

Update copy format:

```typescript
const speaker = t.is_overlap
  ? 'Multiple speakers'
  : (t.speaker_label || undefined);
const speakerPrefix = speaker ? ` ${speaker}:` : '';
return `${formatTime(t.audio_start_time)}${speakerPrefix} ${t.text}`;
```

- [ ] **Step 3: Pass fields into virtualized segments**

In `frontend/src/app/_components/TranscriptPanel.tsx`, extend mapped segment:

```typescript
speaker_id: t.speaker_id,
speaker_label: t.speaker_label,
speaker_color: t.speaker_color,
is_overlap: t.is_overlap,
diarization_status: t.diarization_status,
diarization_method: t.diarization_method,
diarization_confidence: t.diarization_confidence,
```

- [ ] **Step 4: Render speaker label and status badges**

In `frontend/src/components/VirtualizedTranscriptView.tsx`, import:

```tsx
import { DiarizationStatus } from "@/types/diarization";
```

Extend `TranscriptSegment` props:

```typescript
speakerLabel?: string | null;
speakerColor?: string | null;
isOverlap?: boolean | null;
diarizationStatus?: DiarizationStatus | null;
```

Pass values from `segment` to `TranscriptSegment`.

Inside `TranscriptSegment`, before transcript text, render:

```tsx
{(speakerLabel || isOverlap || diarizationStatus) && (
  <div className="mb-1 flex items-center gap-2 text-xs">
    <span
      className="font-medium"
      style={{ color: speakerColor || (isOverlap ? '#f97316' : '#475569') }}
    >
      {isOverlap ? 'Multiple speakers' : speakerLabel}
    </span>
    {diarizationStatus === 'provisional' && (
      <span className="rounded-full bg-amber-100 px-2 py-0.5 font-medium text-amber-800">
        Provisional
      </span>
    )}
    {diarizationStatus === 'final' && (
      <span className="rounded-full bg-green-100 px-2 py-0.5 font-medium text-green-800">
        Final
      </span>
    )}
    {diarizationStatus === 'needs_review' && (
      <span className="rounded-full bg-purple-100 px-2 py-0.5 font-medium text-purple-800">
        Needs review
      </span>
    )}
  </div>
)}
```

- [ ] **Step 5: Run frontend verification**

Run after dependencies are intentionally installed:

```bash
cd frontend
npm run build
```

Expected: TypeScript and Next build pass.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/types/index.ts frontend/src/app/_components/TranscriptPanel.tsx frontend/src/components/VirtualizedTranscriptView.tsx frontend/src/contexts/TranscriptContext.tsx
git commit -m "feat: render speaker labels in transcripts"
```

## Task 9: Apply diarization segment assignments command

**Files:**

- Modify: `frontend/src-tauri/src/database/repositories/diarization.rs`
- Modify: `frontend/src-tauri/src/diarization/commands.rs`

- [ ] **Step 1: Add repository method**

Append to `impl DiarizationRepository`:

```rust
pub async fn update_transcript_assignment(
    pool: &SqlitePool,
    transcript_id: &str,
    assignment: &crate::diarization::types::SpeakerAssignment,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE transcripts SET
            speaker_id = ?,
            speaker_label = ?,
            speaker_color = ?,
            is_overlap = ?,
            diarization_status = ?,
            diarization_method = ?,
            diarization_confidence = ?
         WHERE id = ?"
    )
    .bind(&assignment.speaker_id)
    .bind(&assignment.speaker_label)
    .bind(&assignment.speaker_color)
    .bind(assignment.is_overlap as i64)
    .bind(serde_json::to_value(assignment.diarization_status).unwrap_or_else(|_| serde_json::json!("none")).as_str().unwrap_or("none"))
    .bind(&assignment.diarization_method)
    .bind(assignment.diarization_confidence)
    .bind(transcript_id)
    .execute(pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 2: Add command DTO and command**

Append to `frontend/src-tauri/src/diarization/commands.rs`:

```rust
use crate::diarization::alignment::assign_speaker_to_transcript;
use crate::diarization::types::{SpeakerSegment, TranscriptWindow};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDiarizationRequest {
    pub meeting_id: String,
    pub method: String,
    pub segments: Vec<SpeakerSegment>,
}

#[tauri::command]
pub async fn apply_diarization_segments<R: Runtime>(
    state: State<'_, AppState>,
    request: ApplyDiarizationRequest,
) -> Result<(), String> {
    let pool = state.db_manager.pool();

    let transcripts = sqlx::query_as::<_, crate::database::models::Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC"
    )
    .bind(&request.meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    for transcript in transcripts {
        let window = TranscriptWindow {
            transcript_id: transcript.id.clone(),
            audio_start_time: transcript.audio_start_time,
            audio_end_time: transcript.audio_end_time,
        };

        let assignment = assign_speaker_to_transcript(
            &window,
            &request.segments,
            0.1,
            &request.method,
        );

        DiarizationRepository::update_transcript_assignment(pool, &transcript.id, &assignment)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}
```

- [ ] **Step 3: Register command**

In `frontend/src-tauri/src/lib.rs`, add:

```rust
diarization::commands::apply_diarization_segments,
```

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test -p meetily diarization::alignment::tests
```

Expected: alignment tests still pass.

- [ ] **Step 5: Run compile check**

Run:

```bash
cargo check -p meetily
```

Expected: command compiles when environment baseline is fixed.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/diarization.rs frontend/src-tauri/src/diarization/commands.rs frontend/src-tauri/src/lib.rs
git commit -m "feat: apply diarization segments to transcripts"
```

## Task 10: Self-review and feature boundary check

**Files:**

- Review: all files changed in this plan.

- [ ] **Step 1: Inspect changed files**

Run:

```bash
git status --short
git log --oneline --decorate -10
```

Expected: only intentional feature files changed, with commits per task.

- [ ] **Step 2: Search for placeholder markers**

Run:

```bash
rg -n "T[B]D|T[O]DO|FI[X]ME|P[L]ACEHOLDER|te[m]porary|ha[c]k" frontend/src-tauri/src/diarization frontend/src/components/SpeakerDiarizationSettings.tsx frontend/src/types/diarization.ts
```

Expected: no matches.

- [ ] **Step 3: Run backend verification**

Run when full Xcode is active:

```bash
cargo test -p meetily diarization::types::tests diarization::alignment::tests
cargo check -p meetily
```

Expected: tests pass and crate checks.

- [ ] **Step 4: Run frontend verification**

Run only after frontend dependencies are intentionally installed in a controlled way:

```bash
cd frontend
npm run build
```

Expected: build passes.

- [ ] **Step 5: Document deferred work**

Add a short note to the existing speaker diarization spec under rollout/follow-up if it is not already explicit:

```markdown
Follow-up implementation plans:

- FluidAudio macOS sidecar and model packaging.
- Source-aware microphone/system audio stem persistence.
- Live provisional diarization event stream.
- Offline post-call refinement orchestration.
- Speaker review editor actions backed by database updates.
```

- [ ] **Step 6: Commit documentation note if changed**

```bash
git add docs/superpowers/specs/2026-06-26-speaker-diarization-design.md
git commit -m "docs: note diarization follow-up implementation slices"
```

Skip this commit if the spec already states the follow-up boundary clearly.

## Self-review checklist

Spec coverage:

- Local-first diarization metadata: covered by Tasks 1, 3, 4, 9.
- No hard participant cap: preserved by segment-based data model; no max speaker count added.
- Live labels provisional: represented in status types, settings, and UI badges.
- Post-call final labels: represented in status types and command API; actual offline engine deferred.
- Overlap as `Multiple speakers`: covered by Tasks 1, 2, 8.
- Settings/design-system controls: covered by Tasks 6 and 7.
- Dense-call manual review: data/status foundation included; full review editor deferred.

Intentional gaps for next implementation plan:

- FluidAudio sidecar.
- Real online/offline diarization execution.
- Separate audio source persistence.
- Merge/split/rename commands and review panel.
- Summary prompt integration with final diarization.

Verification constraints:

- Rust verification requires full Xcode because current macOS baseline fails in `cidre`.
- Frontend verification requires a deliberate dependency install strategy because no lockfile is committed.
