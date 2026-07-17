-- Memento long-term learning loop.
--
-- The schema deliberately separates immutable observations/assertions from derived
-- profiles and suggestions. Model predictions are operational evidence only; they do
-- not become trusted training labels without an explicit review action.

ALTER TABLE meetings ADD COLUMN meeting_type TEXT NOT NULL DEFAULT 'uncertain'
    CHECK (meeting_type IN (
        'uncertain', 'general', 'standup', 'planning', 'project_sync',
        'one_on_one', 'interview', 'client_sync', 'technical_deep_dive'
    ));
ALTER TABLE meetings ADD COLUMN meeting_type_confidence REAL
    CHECK (meeting_type_confidence IS NULL OR
           (meeting_type_confidence >= 0.0 AND meeting_type_confidence <= 1.0));
ALTER TABLE meetings ADD COLUMN meeting_type_review_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (meeting_type_review_status IN ('pending', 'accepted', 'rejected'));

ALTER TABLE transcripts ADD COLUMN raw_transcript TEXT;
ALTER TABLE transcripts ADD COLUMN transcript_version INTEGER NOT NULL DEFAULT 1
    CHECK (transcript_version >= 1);
UPDATE transcripts SET raw_transcript = transcript WHERE raw_transcript IS NULL;

ALTER TABLE speakers ADD COLUMN profile_version INTEGER NOT NULL DEFAULT 0
    CHECK (profile_version >= 0);
ALTER TABLE speakers ADD COLUMN learning_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (learning_enabled IN (0, 1));
ALTER TABLE speakers ADD COLUMN consent_state TEXT NOT NULL DEFAULT 'pending'
    CHECK (consent_state IN ('pending', 'granted', 'denied', 'revoked'));
ALTER TABLE speakers ADD COLUMN deleted_at TEXT;

ALTER TABLE collections ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0
    CHECK (is_system IN (0, 1));
ALTER TABLE collections ADD COLUMN system_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_collections_system_key
    ON collections(system_key) WHERE system_key IS NOT NULL;

INSERT INTO collections(name, kind, is_system, system_key)
SELECT 'Memento Inbox', 'manual', 1, 'inbox'
WHERE NOT EXISTS (SELECT 1 FROM collections WHERE system_key = 'inbox')
  AND NOT EXISTS (SELECT 1 FROM collections WHERE name = 'Memento Inbox');

CREATE TABLE IF NOT EXISTS meeting_type_suggestions (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    suggested_type TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    explanation_json TEXT NOT NULL DEFAULT '[]',
    model_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'superseded')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_meeting_type_suggestions_review
    ON meeting_type_suggestions(status, created_at DESC);

CREATE TABLE IF NOT EXISTS collection_suggestions (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    collection_id INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    suggested_name TEXT,
    suggestion_kind TEXT NOT NULL CHECK (suggestion_kind IN ('collection', 'series')),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    explanation_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'superseded')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_collection_suggestions_review
    ON collection_suggestions(status, created_at DESC);

-- Universal append-only evidence ledger. Payloads contain normalized, bounded
-- structural data; transcript/audio content stays in its purpose-specific store.
CREATE TABLE IF NOT EXISTS learning_events (
    id INTEGER PRIMARY KEY,
    event_uuid TEXT NOT NULL UNIQUE,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
    capture_session_id TEXT REFERENCES capture_sessions(id) ON DELETE SET NULL,
    event_kind TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'model', 'policy', 'system')),
    trust_tier TEXT NOT NULL CHECK (trust_tier IN ('untrusted', 'operational', 'trusted')),
    scope TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    model_version TEXT,
    schema_version TEXT NOT NULL DEFAULT 'learning_v1',
    consent_scope TEXT NOT NULL DEFAULT 'local',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_learning_events_meeting_created
    ON learning_events(meeting_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_learning_events_target
    ON learning_events(target_type, target_id, created_at DESC);

CREATE TABLE IF NOT EXISTS speaker_clusters (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    diarization_run_id TEXT NOT NULL,
    local_cluster_id INTEGER NOT NULL,
    placeholder_speaker_id INTEGER REFERENCES speakers(id) ON DELETE SET NULL,
    operational_speaker_id INTEGER REFERENCES speakers(id) ON DELETE SET NULL,
    embedding BLOB,
    speech_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (speech_duration_ms >= 0),
    speech_quality REAL CHECK (speech_quality IS NULL OR
        (speech_quality >= 0.0 AND speech_quality <= 1.0)),
    overlap_ratio REAL CHECK (overlap_ratio IS NULL OR
        (overlap_ratio >= 0.0 AND overlap_ratio <= 1.0)),
    channel_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (channel_kind IN ('microphone', 'system', 'mixed', 'unknown')),
    model_version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, diarization_run_id, local_cluster_id)
);
CREATE INDEX IF NOT EXISTS idx_speaker_clusters_meeting
    ON speaker_clusters(meeting_id, created_at DESC);

CREATE TABLE IF NOT EXISTS speaker_cluster_segments (
    cluster_id INTEGER NOT NULL REFERENCES speaker_clusters(id) ON DELETE CASCADE,
    transcript_id TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
    overlap_ratio REAL NOT NULL CHECK (overlap_ratio >= 0.0 AND overlap_ratio <= 1.0),
    PRIMARY KEY(cluster_id, transcript_id)
);
CREATE INDEX IF NOT EXISTS idx_speaker_cluster_segments_transcript
    ON speaker_cluster_segments(transcript_id);

CREATE TABLE IF NOT EXISTS identity_inference_runs (
    id INTEGER PRIMARY KEY,
    cluster_id INTEGER NOT NULL REFERENCES speaker_clusters(id) ON DELETE CASCADE,
    voice_model_version TEXT NOT NULL,
    fusion_model_version TEXT NOT NULL,
    candidate_scores_json TEXT NOT NULL DEFAULT '[]',
    policy_result TEXT NOT NULL CHECK (policy_result IN ('auto_assign', 'confirm', 'unknown')),
    explanation_factors_json TEXT NOT NULL DEFAULT '[]',
    top_score REAL,
    top_margin REAL,
    policy_snapshot_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_identity_runs_cluster
    ON identity_inference_runs(cluster_id, created_at DESC);

CREATE TABLE IF NOT EXISTS identity_assertions (
    id INTEGER PRIMARY KEY,
    assertion_uuid TEXT NOT NULL UNIQUE,
    cluster_id INTEGER NOT NULL REFERENCES speaker_clusters(id) ON DELETE CASCADE,
    speaker_id INTEGER REFERENCES speakers(id) ON DELETE SET NULL,
    polarity TEXT NOT NULL CHECK (polarity IN ('positive', 'negative', 'unknown')),
    scope TEXT NOT NULL CHECK (scope IN ('segment', 'cluster', 'meeting', 'global')),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'model', 'policy', 'system')),
    trust_tier TEXT NOT NULL CHECK (trust_tier IN ('untrusted', 'operational', 'trusted')),
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    reason TEXT NOT NULL,
    model_version TEXT,
    supersedes_id INTEGER REFERENCES identity_assertions(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_identity_assertions_cluster
    ON identity_assertions(cluster_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_identity_assertions_speaker
    ON identity_assertions(speaker_id, trust_tier, created_at DESC);

CREATE TABLE IF NOT EXISTS voice_samples (
    id INTEGER PRIMARY KEY,
    speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    cluster_id INTEGER NOT NULL REFERENCES speaker_clusters(id) ON DELETE CASCADE,
    assertion_id INTEGER NOT NULL REFERENCES identity_assertions(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0),
    speech_quality REAL NOT NULL CHECK (speech_quality >= 0.0 AND speech_quality <= 1.0),
    overlap_ratio REAL NOT NULL CHECK (overlap_ratio >= 0.0 AND overlap_ratio <= 1.0),
    channel_kind TEXT NOT NULL,
    eligibility TEXT NOT NULL CHECK (eligibility IN ('trusted', 'rejected', 'excluded')),
    exclusion_reason TEXT,
    model_version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(speaker_id, cluster_id, assertion_id)
);
CREATE INDEX IF NOT EXISTS idx_voice_samples_speaker_eligibility
    ON voice_samples(speaker_id, eligibility, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_voice_samples_cluster
    ON voice_samples(cluster_id);
CREATE INDEX IF NOT EXISTS idx_voice_samples_assertion
    ON voice_samples(assertion_id);

CREATE TABLE IF NOT EXISTS voice_centroids (
    id INTEGER PRIMARY KEY,
    speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    profile_version INTEGER NOT NULL CHECK (profile_version >= 1),
    mode_index INTEGER NOT NULL CHECK (mode_index >= 0),
    embedding BLOB NOT NULL,
    dispersion REAL NOT NULL DEFAULT 0.0 CHECK (dispersion >= 0.0),
    sample_count INTEGER NOT NULL CHECK (sample_count >= 1),
    channel_hint TEXT,
    model_version TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(speaker_id, profile_version, mode_index)
);
CREATE INDEX IF NOT EXISTS idx_voice_centroids_active
    ON voice_centroids(speaker_id, is_active, profile_version DESC);

CREATE TABLE IF NOT EXISTS speaker_profile_versions (
    id INTEGER PRIMARY KEY,
    speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    parent_version INTEGER,
    build_reason TEXT NOT NULL,
    evidence_cutoff_id INTEGER,
    snapshot_json TEXT NOT NULL,
    published_embedding BLOB NOT NULL,
    model_version TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(speaker_id, version)
);

CREATE TABLE IF NOT EXISTS transcript_corrections (
    id INTEGER PRIMARY KEY,
    correction_uuid TEXT NOT NULL UNIQUE,
    transcript_id TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    previous_text TEXT NOT NULL,
    corrected_text TEXT NOT NULL,
    previous_version INTEGER NOT NULL CHECK (previous_version >= 1),
    new_version INTEGER NOT NULL CHECK (new_version > previous_version),
    actor_kind TEXT NOT NULL DEFAULT 'user' CHECK (actor_kind IN ('user', 'system')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_transcript_corrections_meeting
    ON transcript_corrections(meeting_id, created_at DESC);

CREATE TABLE IF NOT EXISTS terminology_terms (
    id INTEGER PRIMARY KEY,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'collection', 'series')),
    scope_id INTEGER,
    canonical TEXT NOT NULL,
    normalized_canonical TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'confirmed', 'rejected')),
    confidence REAL NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    support_count INTEGER NOT NULL DEFAULT 0 CHECK (support_count >= 0),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    confirmed_at TEXT,
    UNIQUE(scope_kind, scope_id, normalized_canonical)
);
CREATE INDEX IF NOT EXISTS idx_terminology_terms_review
    ON terminology_terms(status, support_count DESC, last_seen_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_terminology_terms_scope_unique
    ON terminology_terms(scope_kind, COALESCE(scope_id, -1), normalized_canonical);

CREATE TABLE IF NOT EXISTS terminology_term_versions (
    id INTEGER PRIMARY KEY,
    term_id INTEGER NOT NULL REFERENCES terminology_terms(id) ON DELETE CASCADE,
    previous_canonical TEXT NOT NULL,
    new_canonical TEXT NOT NULL,
    previous_status TEXT NOT NULL,
    new_status TEXT NOT NULL,
    previous_version INTEGER NOT NULL CHECK (previous_version >= 1),
    new_version INTEGER NOT NULL CHECK (new_version > previous_version),
    actor_kind TEXT NOT NULL DEFAULT 'user' CHECK (actor_kind IN ('user', 'system')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(term_id, new_version)
);

CREATE TABLE IF NOT EXISTS terminology_aliases (
    id INTEGER PRIMARY KEY,
    term_id INTEGER NOT NULL REFERENCES terminology_terms(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'confirmed', 'rejected')),
    support_count INTEGER NOT NULL DEFAULT 1 CHECK (support_count >= 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(term_id, normalized_alias)
);

CREATE TABLE IF NOT EXISTS terminology_evidence (
    id INTEGER PRIMARY KEY,
    term_id INTEGER NOT NULL REFERENCES terminology_terms(id) ON DELETE CASCADE,
    alias_id INTEGER REFERENCES terminology_aliases(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    transcript_id TEXT REFERENCES transcripts(id) ON DELETE SET NULL,
    correction_id INTEGER REFERENCES transcript_corrections(id) ON DELETE SET NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('correction', 'repetition', 'user')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(term_id, meeting_id, transcript_id, source_kind)
);

CREATE TABLE IF NOT EXISTS context_edges (
    id INTEGER PRIMARY KEY,
    edge_type TEXT NOT NULL CHECK (edge_type IN ('coattendance', 'series_attendance', 'response')),
    source_speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    target_speaker_id INTEGER REFERENCES speakers(id) ON DELETE CASCADE,
    collection_id INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    support_count INTEGER NOT NULL CHECK (support_count >= 1),
    weight REAL NOT NULL CHECK (weight >= 0.0 AND weight <= 1.0),
    valid_from TEXT NOT NULL DEFAULT (datetime('now')),
    valid_to TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(edge_type, source_speaker_id, target_speaker_id, collection_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_context_edges_null_safe_unique
    ON context_edges(
        edge_type,
        source_speaker_id,
        COALESCE(target_speaker_id, -1),
        COALESCE(collection_id, -1)
    );

CREATE TABLE IF NOT EXISTS language_profiles (
    speaker_id INTEGER PRIMARY KEY REFERENCES speakers(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    support_meetings INTEGER NOT NULL DEFAULT 0 CHECK (support_meetings >= 0),
    features_json TEXT NOT NULL DEFAULT '{}',
    model_version TEXT NOT NULL DEFAULT 'language_shadow_v1',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS conversation_dynamics_profiles (
    speaker_id INTEGER PRIMARY KEY REFERENCES speakers(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    support_meetings INTEGER NOT NULL DEFAULT 0 CHECK (support_meetings >= 0),
    features_json TEXT NOT NULL DEFAULT '{}',
    model_version TEXT NOT NULL DEFAULT 'dynamics_shadow_v1',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS reconciliation_runs (
    id INTEGER PRIMARY KEY,
    run_uuid TEXT NOT NULL UNIQUE,
    trigger_kind TEXT NOT NULL,
    trigger_ref TEXT,
    input_snapshot_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed', 'partially_applied', 'applied', 'rejected', 'failed', 'rolled_back')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS reconciliation_suggestions (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES reconciliation_runs(id) ON DELETE CASCADE,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    suggestion_kind TEXT NOT NULL,
    previous_value_json TEXT NOT NULL,
    proposed_value_json TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    evidence_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'applied', 'rolled_back')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_reconciliation_suggestions_review
    ON reconciliation_suggestions(status, created_at DESC);

CREATE TABLE IF NOT EXISTS artifact_versions (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    artifact_kind TEXT NOT NULL CHECK (artifact_kind IN (
        'transcript', 'identity', 'glossary', 'summary', 'embedding', 'classification'
    )),
    version INTEGER NOT NULL CHECK (version >= 1),
    source_versions_json TEXT NOT NULL DEFAULT '{}',
    artifact_ref TEXT,
    stale INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, artifact_kind, version)
);

CREATE TABLE IF NOT EXISTS quality_observations (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
    capture_session_id TEXT REFERENCES capture_sessions(id) ON DELETE CASCADE,
    component TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    cohort_json TEXT NOT NULL DEFAULT '{}',
    model_version TEXT,
    schema_version TEXT NOT NULL DEFAULT 'quality_v1',
    computed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_quality_observations_metric
    ON quality_observations(component, metric_name, computed_at DESC);
