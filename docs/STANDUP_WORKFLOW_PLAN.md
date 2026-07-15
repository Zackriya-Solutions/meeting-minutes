# Standup workflow plan

## Product outcome

Turn a standup recording into an evidence-backed operating loop, not just a prose summary. The product should help the team prepare, run a focused status round, preserve decisions and actions, and understand what changed across a recurring series without becoming an employee-surveillance tool.

## Audit of the recent requests

| Request | Current status | Missing work |
| --- | --- | --- |
| Improve the weak standup template | Standup V2 pipeline is being implemented | Real-provider and held-out corpus evaluation |
| Use the 11:00 standups and 17:30 project/productivity meetings | Batch import is running; filename classes are weak labels | Content classification and reviewed labels for all recordings |
| Use the additional archive and earlier files | 50 unique local recordings prepared; duplicate earlier files removed by SHA-256 | Finish transcription and final integrity report |
| Rename archive files by date/time | Done only where evidence supports the date; unknown dates remain explicit | Optional user review for the 35 unknown dates |
| Avoid per-file UI import | PR #23 adds one-folder Tauri batch import, picker-free Tauri command, console startup mode, hash dedupe, resume, and JSON report | Finish the 50-file run and publish the integrity report |
| Learn from roughly 50 meetings | Corpus/evaluation plan exists | Gold labels, frozen split, evaluation runner, feedback capture |
| Infer speaker names and aliases safely | PR #27 adds a local candidate store, abuse filtering, evidence UI, confirmation, aliases, and salted rejection fingerprints | Evaluate precision after diarization is available on the corpus |
| Keep DeepSeek summaries reliable | PR #21 implements bounded generation and direct Russian output | Merge/review and Standup V2 provider evaluation |
| Automatic meeting detection | PR #22 is open | Merge/review and false-positive/false-negative evaluation |

The database previously treated import time as meeting time. The standup-series slice adds a
separate `occurred_at` value and backfills it only from the safely normalized
`YYYY-MM-DD_HH-MM_...` titles. Unknown source dates remain unknown and fall back to `created_at`;
filesystem modification time is never silently promoted to meeting truth.

The series-digest slice builds its weekly/sprint view deterministically from accepted records.
It is anchored to the newest meeting in the series rather than today's date, so historical imports
remain useful. Pending and rejected records never become facts; pending coverage stays visible.

## Before the standup

### Pre-brief

- show open actions from the previous standup with owner, age, and evidence;
- show unresolved blockers and decisions that require confirmation;
- let each participant add a short planned update locally before the call;
- suggest an agenda and a parking-lot list from the recurring series;
- identify missing context, not “underperforming participants”.

### Meeting-type and series selection

- suggest standup only from cadence, title, calendar/app signals, and reviewed history;
- allow one-click correction because filename/time is not ground truth;
- attach the meeting to a series before generation so carry-forward items are available.

## During the standup

### Status-round assistance

- detect completed/recent work, next work, and blockers from meaning rather than keywords;
- show attribution confidence and keep uncertain updates unattributed;
- capture explicit decisions and actions with timestamps;
- detect when the status round becomes a long technical deep dive and move it to a separate section;
- offer a parking-lot marker and time-box warning without interrupting recording.

### Useful live views

- compact “who has an update” checklist based on explicit speech evidence;
- blocker/dependency map between people or projects;
- unresolved question list;
- private personal scratchpad that is never treated as transcript evidence.

## After the standup

### Primary artifact: Standup V2

- outcome;
- per-participant completed/recent, next, and blockers;
- decisions;
- action items with explicit owner/due or unknown/not stated;
- risks and impact;
- deep dives and parking lot;
- useful unattributed facts;
- a clickable evidence timestamp for every record.

### Follow-up workflows

- review/accept/edit/reject each extracted record before it affects the series;
- convert accepted actions to the existing action-item lifecycle;
- copy a concise team digest or a personal digest;
- export Markdown/JSON for Jira, Linear, Slack, or email later, with explicit outbound consent;
- schedule a reminder only for an accepted action with an owner and due date.

## Across a recurring series

- carry open actions forward and mark done/cancelled/superseded;
- show commitments that changed since the previous standup;
- identify aging or recurring blockers;
- deduplicate the same technical deep dive across meetings;
- maintain a decision log and parking-lot backlog;
- summarize project movement over a week or sprint with citations;
- support “what changed since I was away?” and handoff views;
- surface terminology/custom-vocabulary candidates from repeated ASR corrections.

The first implemented digest includes 7/14/30-day and all-history windows, accepted highlights,
participant updates, open/completed actions, decisions, risks, and deep dives. Every row links to
its source meeting and transcript time. Cancelled actions stay in the JSON result for auditability
but are not promoted in the primary UI or Markdown.

Do not produce productivity scores, speaking-time rankings, sentiment scores for people, or automatic performance judgments. These are easy to misuse and are not necessary for making standups more useful.

## Quality and learning loop

1. Classify each recording by reviewed content: pure status, status plus deep dive, planning/sync, one-to-one, general, or uncertain.
2. Label evidence records, not only final prose.
3. Freeze train/dev/test by meeting series, never by chunks.
4. Compare providers and prompt/schema versions on the same held-out meetings.
5. Track coverage, action precision/recall, attribution precision, unsupported claims, timestamp validity, duplicate rate, unknown calibration, latency, and provider failures.
6. Save user corrections as local labels with schema/prompt version and consent state.
7. Consider fine-tuning only after a stable schema and a sufficiently large reviewed set reveal a persistent measurable gap.

Initial gates: zero invalid timestamps, no silently invented owner/due date, at least 95% owner precision when shown, below 2% unsupported decisions/actions, and visible failure on invalid provider output.

## Speaker-name and alias safety

- names from speech are untrusted candidates, never direct profile updates;
- require self-introduction, explicit introduction, or repeated direct-address/response evidence;
- reject profanity, insults, roles, generic nouns, control characters, and implausible name shapes;
- store abusive rejected strings only as a salted fingerprint plus reason when possible;
- keep aliases on a person entity and never overwrite a confirmed display name;
- require repeated linguistic plus voice evidence before a cross-meeting merge suggestion;
- show the exact evidence moment and require confirmation for every identity change;
- keep processing local unless the user explicitly enables a cloud provider.

## Delivery sequence

1. Merge PR #21 (summary reliability) and PR #23 (batch import); keep PR #22 independently reviewable.
2. Ship Standup V2 schema, chunk extraction, conservative merge, deterministic renderer, and stored structured result.
3. Finish corpus import, integrity validation, content classification, and a 12-15 meeting gold set.
4. Add record-level review/evaluation UI and provider comparison.
5. Add action carry-forward, pre-brief, parking lot, and weekly/sprint digest on top of reviewed series.
6. Add safe speaker-name candidates and alias confirmation.
7. Fix the ONNX `token_type_ids` regression before depending on RAG for cross-meeting output.
