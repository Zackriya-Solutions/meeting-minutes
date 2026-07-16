# Standup workflow plan

## Product outcome

Turn a standup recording into an evidence-backed operating loop, not just a prose summary. The product should help the team prepare, run a focused status round, preserve decisions and actions, and understand what changed across a recurring series without becoming an employee-surveillance tool.

## Audit of the recent requests

| Request | Current status | Missing work |
| --- | --- | --- |
| Improve the weak standup template | Standup V2, review records, series views, pre-brief, live assistance, and template suggestion are implemented in the open stack | Precision/latency improvements and release-quality held-out evaluation |
| Use the 11:00 standups and 17:30 project/productivity meetings | 19 meetings are manually content-classified; a six-meeting leakage-safe progress set includes three standups and three contrasts | Expand independent gold and add more complete pure-status series |
| Use the additional archive and earlier files | Import verified: 50 unique manifest hashes, no duplicates/extras, 0 failures, and non-empty transcripts | Keep future imports idempotent and review low-coverage recordings |
| Rename archive files by date/time | Done only where evidence supports the date; unknown dates remain explicit | Optional user review for the 35 unknown dates |
| Avoid per-file UI import | PR #23 adds one-folder Tauri batch import, picker-free Tauri command, console startup mode, hash dedupe, resume, and JSON report; the 50-file run is complete | Merge/review and keep reports privacy-safe |
| Learn from roughly 50 meetings | All 52 transcript-bearing local meetings were exported privately; the runner and overlays are implemented | Grow transcript-only gold, measure corrections, and avoid one-class model labels |
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

The preparation-notes slice implements meeting-local planned updates, parking-lot topics, and a
private scratchpad. They are stored separately from transcripts and generated records, never sent
to the summary model, and can be completed, reopened, or archived by the user. This separation is
intentional: a private thought must not silently become a claim that somebody said during the call.

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

### Measured local baseline

The first local Qwen 3.5 4B run is diagnostic evidence, not a release result. On three reviewed
standups it reached `0.50` fact coverage and `0.373` action F1, while `72.6%` of extracted
decisions/actions were unsupported and p95 latency was about 229 seconds. Adding three reviewed
contrast meetings removed all protocol errors but exposed the more important failure mode: the
forced Standup template produced 32 false-positive records, including 22 actions. Aggregate action
F1 fell to `0.289`, unsupported decisions/actions rose to `80.7%`, and p95 reached 609 seconds.

Therefore the next optimization target is precision, especially action/decision gating and
meeting-type suppression. The deterministic suggestion logic in PR #33 selected the intended
template for all six progress samples, including the misleading-title contrast, because a real
status-round hand-off is required. That small result validates the safety shape, not the release
false-positive rate. Do not trade it for higher recall, auto-select Standup from a filename, or
weaken the release thresholds. Suggestions remain confirm-before-generate until a larger held-out
set is measured and acceptable.

A cloned-database Qwen experiment then required every candidate to provide a short verbatim quote
instead of letting the application hydrate evidence from a timestamped line. It reduced scored
outputs from 173 to 78 and improved action precision from `0.206` to `0.316`, but fact coverage
collapsed from `0.50` to `0.185`, action recall from `0.483` to `0.207`, action F1 from
`0.289` to `0.250`, and the overall unsupported rate increased from `0.844` to `0.872`.
This candidate must not ship as the precision fix. Verbatim evidence remains useful, but the next
iteration needs an explicit semantic claim/category verifier and must preserve supported recall.
A first same-model verifier prototype was also rejected after the first held-out standup: it kept
22 records, including obvious category errors, while increasing that meeting's latency from about
49 to 149 seconds. Do not add an expensive Qwen self-review pass without evidence that its
precision is independently calibrated; prioritize conservative template gating, human review, and
a genuinely independent entailment signal.

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

1. Bring the existing PR stack through review without merging it implicitly; land summary reliability,
   batch import, Standup V2, review/series, and corpus evaluation in dependency order.
2. Validate the conservative meeting-type suggestion on the expanded corpus, add a precision-first
   Standup extraction iteration, and rerun the same six-meeting set before expanding features.
3. Expand to 12–15 independently annotated meetings without series leakage. At least two more
   complete pure-status recordings must come from a new corpus rather than relabelling deep dives.
4. Ship the already-implemented review, carry-forward, pre-brief, live, digest, proactive-insight,
   and safe-name slices only after their reviewer checks are green.
5. Fix/merge the ONNX `token_type_ids` regression before relying on RAG or collections for
   cross-meeting answers; then evaluate citation precision on held-out series.
6. Add calendar/notes context and outbound follow-up integrations later, with explicit consent and
   no automatic employee scoring or external actions.
