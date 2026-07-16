# Standup summary quality plan

## Objective

Make standup summaries dependable on real Russian team meetings, including meetings that start as a status round and then turn into a long technical discussion. The output must preserve evidence, avoid invented ownership, and remain useful when diarization or names are uncertain.

## Current product position

The supported Tauri application already has the foundations needed for this work: local meeting storage and transcription, summary templates, diarization and speaker profiles, privacy quality gates, collections, and RAG indexing. The remaining work is mostly about reliability and evidence quality rather than adding another parallel backend.

The active dependency chain is:

1. PR #21: reliable bounded summarization and correct DeepSeek behavior.
2. PR #22: automatic meeting detection, so the evaluation corpus reflects meetings that the product would actually capture.
3. This batch-import change: repeatable ingestion, content-hash deduplication, and corpus validation.
4. Standup V2 and its evaluation loop.

The ONNX embedding path currently fails when the model expects `token_type_ids`. Search/RAG and collection digests should therefore remain behind a quality warning until that regression is fixed and a real imported-meeting smoke test passes.

## Evaluation corpus

The initial local corpus contains 50 unique recordings (about 28.74 hours): 17 filename-classified standups and 33 other meetings that can be used as negative or cross-template examples. Dates are retained only when supported by filename or filesystem evidence; unknown dates are not guessed. Exact source paths and transcript text are excluded from the corpus manifest.

The filename category is a weak label. After transcription, each recording should be classified into one of:

- pure status round;
- status round plus deep dive;
- planning or project sync;
- one-to-one;
- general meeting;
- uncertain.

The first five imported standup-labelled recordings already show why keyword prompting is insufficient. Across 473 transcript segments, literal forms of “yesterday” were present in only two segments, “today” in four, and “blocker” in none. Status, intent, and impediments must therefore be inferred from evidence and dialogue structure, while still allowing the model to return `not stated`.

Evaluation splits must be made by meeting or recurring series, never by transcript chunk, so near-duplicate conversations cannot leak across train and test sets.

### Corpus lifecycle

1. Preserve the original audio as a read-only local source; do not add audio, transcripts, absolute paths, or participant names to Git.
2. Import by folder through the Tauri command surface, sequentially, with SHA-256 deduplication and per-file failure reporting.
3. Validate transcript timestamps, source hashes, meeting metadata, and non-empty speech coverage after import.
4. Assign a reviewed meeting type and a recurring-series identifier where known.
5. Label evidence records rather than editing only the final prose summary.
6. Freeze a held-out test split before prompt tuning begins.

For the first iteration, label 12-15 meetings: at least six standups, three mixed status/deep-dive meetings, and three non-standups. Expand only after the labeling rubric produces consistent decisions between review passes.

## Standup V2 output contract

Generate a structured intermediate result before Markdown rendering:

1. Short outcome-oriented overview.
2. Per-participant updates: completed/recent work, next work, blockers, and evidence timestamps.
3. Decisions: decision, rationale if explicit, participants, and evidence.
4. Action items: task, explicit owner or `unknown`, explicit due date or `not stated`, and evidence.
5. Risks and blockers: impact and owner only when stated.
6. Deep dives / parking lot: technical discussions that should not overwrite the status round.
7. Unattributed facts: useful content that cannot safely be assigned to a participant.

The renderer should deterministically turn this structure into Markdown. Empty fields must say that the information was not stated; the model must not fill them from plausibility.

## Generation pipeline

1. Use the reliable bounded/chunked summary path from PR #21.
2. Preserve timestamps and speaker IDs in every chunk.
3. Extract evidence records from each chunk rather than free-form chunk summaries.
4. Merge and deduplicate records across chunks.
5. Resolve contradictory records conservatively and keep the stronger evidence.
6. Validate the structured result against a schema.
7. Render the selected meeting template.

This avoids the current failure mode where a narrative chunk summary loses attribution before the final template pass.

### Structured extraction record

Each extracted item should retain `kind`, `text`, `speaker_id`, `owner_status`, `due_date_status`, `start_seconds`, `end_seconds`, `source_chunk_id`, and `confidence`. `speaker_id`, owner, and due date are nullable, but the reason for uncertainty must be explicit. The final merge stage may combine two records only when their evidence ranges and semantic content agree.

Provider-specific prompts may adapt syntax, but every provider must return the same versioned schema. DeepSeek, local Ollama, and other cloud providers should be compared on the same frozen examples rather than judged from different meetings.

## Speaker names and aliases

Names mentioned in speech are candidates, not trusted profile updates. A candidate links a meeting speaker cluster to a person entity and stores evidence, confidence, source type, and review status.

Do not send speaker-name candidates through the current generic entity resolver unchanged. Its fuzzy auto-merge threshold is appropriate for low-risk topic/project cleanup, but person identity changes require stricter validation and an explicit candidate/review state.

Strong evidence includes self-introduction, explicit introduction by another participant, and repeated direct address followed by a response from the same voice cluster. A single mention, a third-party reference, a role, a pronoun, or a low-confidence ASR fragment is insufficient.

Safety rules:

- never overwrite a user-confirmed speaker name automatically;
- keep aliases on the person entity, not on a temporary meeting speaker cluster;
- reject profanity, insults, generic nouns, control characters, and implausible name shapes;
- keep conflicting aliases in a review queue;
- require repeated linguistic evidence plus stable voice evidence before suggesting a cross-meeting merge;
- show the supporting transcript moment for every suggestion;
- keep processing local unless the user explicitly enables a cloud provider.

Store rejected candidates only as a salted fingerprint plus rejection reason when possible. Raw abusive text is evidence for moderation, not a reusable alias. Candidate extraction must also normalize grammatical forms and diminutives without treating phonetic similarity alone as identity proof.

## Quality metrics

Create a manually reviewed gold set before any fine-tuning. Prompt and pipeline changes come first.

- coverage of gold status updates, decisions, blockers, and actions;
- owner-attribution precision and recall;
- unsupported-claim rate;
- action-item precision and recall;
- evidence timestamp validity;
- duplicate rate across chunks;
- `unknown` calibration when evidence is insufficient;
- summary latency and provider failure rate.

The first release gate should prioritize low hallucination and high attribution precision over maximum recall.

Initial release gates:

- zero invalid evidence timestamps in the held-out set;
- no silently invented owner or due date;
- unsupported-claim rate below 2% for decisions and actions;
- owner-attribution precision at least 95% when an owner is shown;
- all provider or schema failures surfaced as incomplete results, never as a successful empty summary;
- repeat runs on the same transcript produce structurally equivalent decisions and actions.

## Implementation slices

### Slice A: trustworthy Standup V2

- add the versioned extraction schema and validator;
- implement chunk evidence extraction and deterministic merge/render;
- expose timestamp links from every decision, action, blocker, and participant update;
- separate status updates from deep dives and unattributed material;
- fail visibly on an invalid or incomplete provider response.

### Slice B: evaluation loop

- add a local evaluation runner for stored meetings and saved prompt/schema versions;
- provide a compact review screen for accept/edit/reject at evidence-record level;
- export only de-identified metrics and labels by default;
- compare provider, prompt, latency, and failure-rate results on a frozen split.

### Slice C: speaker identity candidates

- extract self-introduction, explicit introduction, and direct-address candidates;
- link candidates to diarized speaker clusters without changing confirmed profiles;
- add profanity/name-shape filters, alias normalization, evidence display, and review states;
- allow cross-meeting suggestions only after repeated linguistic and voice evidence.

### Slice D: recurring meeting intelligence

- attach meetings to reviewed collections or series;
- carry unresolved actions forward without duplicating them;
- show recurring blockers, changed commitments, and trend summaries;
- enable collection-level RAG only after the embedding regression and retrieval quality gate are fixed.

## Delivery order

1. Finish and merge DeepSeek reliability (PR #21).
2. Finish and merge auto meeting detection (PR #22), because missed recordings invalidate every downstream quality metric.
3. Complete resilient corpus import and data-quality checks.
4. Implement Standup V2 structured extraction, deterministic rendering, and evidence links.
5. Add an evaluation runner and review UI; label the initial gold set.
6. Add safe speaker-name candidate extraction and alias review.
7. Fix the ONNX `token_type_ids` embedding regression before treating semantic search, RAG, or collection digests as production-ready.
8. Use collections/series for cross-meeting action deduplication and recurring-standup trends.
9. Consider fine-tuning only after prompt/pipeline baselines and a sufficiently large reviewed dataset exist.

Fine-tuning is not the first lever. With the current 50-recording corpus, the highest-value use is evaluation, prompt/schema iteration, vocabulary discovery, and failure analysis. Training becomes justified only when there are enough reviewed examples, a stable target schema, a frozen test set, and a measurable gap that retrieval and prompting do not close.
