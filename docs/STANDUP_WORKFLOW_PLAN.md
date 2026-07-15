# Standup workflow plan

## Product outcome

Turn a standup recording into an evidence-backed operating loop, not just a prose summary. The product should help the team prepare, run a focused status round, preserve decisions and actions, and understand what changed across a recurring series without becoming an employee-surveillance tool.

## Audit of the recent requests

| Request | Current status | Missing work |
| --- | --- | --- |
| Improve the weak standup template | PR #24 implements Standup V2; PR #33 adds an explainable local suggestion hardened on a false-title recording | Real-provider and held-out corpus evaluation |
| Use the 11:00 standups and 17:30 project/productivity meetings | 32 of 50 unique recordings are imported; the remaining 18 are running, and filename classes remain weak labels | Content classification and reviewed labels for all recordings |
| Use the additional archive and earlier files | The 17-file priority pass finished with 7 imports, 10 hash-based skips, and 0 failures | Finish the remaining 18 recordings and final integrity report |
| Rename archive files by date/time | Done only where evidence supports the date; unknown dates remain explicit | Optional user review for the 35 unknown dates |
| Avoid per-file UI import | PR #23 adds one-folder Tauri batch import, picker-free Tauri command, console startup mode, hash dedupe, resume, and JSON report | Finish the 50-file run and publish the integrity report |
| Learn from roughly 50 meetings | PR #26 adds the private quality gate and PR #29 the resumable runner; 19 recordings have content review notes | Gold labels, frozen series split, and provider runs |
| Infer speaker names and aliases safely | PR #27 adds a local candidate store, abuse filtering, evidence UI, confirmation, aliases, and salted rejection fingerprints | Evaluate precision after diarization is available on the corpus |
| Keep DeepSeek summaries reliable | PR #21 implements bounded generation and direct Russian output | Merge/review and Standup V2 provider evaluation |
| Automatic meeting detection | PR #22 is open | Merge/review and false-positive/false-negative evaluation |
| Do more than summarize a standup | PRs #25, #28, #30, and #32 add reviewed facts, series digest, private preparation, and live facilitation; PR #34 adds a local evidence-backed insight inbox | Held-out workflow evaluation and explicit outbound integrations later |

The database previously treated import time as meeting time. The standup-series slice adds a
separate `occurred_at` value and backfills it only from the safely normalized
`YYYY-MM-DD_HH-MM_...` titles. Unknown source dates remain unknown and fall back to `created_at`;
filesystem modification time is never silently promoted to meeting truth.

### Boundary with 17:30 project meetings

The first newly imported Gigatool recording is a release/product planning sync, not a standup:
it sets a release target, changes task and sprint scope, discusses tester bugs, and evaluates
product hypotheses. It should use the standard meeting pipeline plus collection-level decision,
action, and hypothesis history. Standup V2 must not absorb every recurring team meeting merely
because it contains status language. This recording also contains routine profanity, which is
real-corpus evidence for the abuse filter and confirmation boundary in speaker-name candidates.

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

The first suggestion slice is deliberately assistive rather than automatic. A strong title,
reviewed standups in the same series, or status-round language can contribute to a Standup V2
prompt. Once a transcript exists, a round hand-off plus status language is required; this protects
against real recordings whose filename says `standup` but whose content is team feedback. Time and
duration alone are insufficient, and explicit planning, one-to-one, retrospective, or interview
titles suppress the prompt. The user must still choose the template; no transcript leaves the
device and no model is called for this decision.

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

## Proactive knowledge layer

The reviewed `proactive-harness` recording adds a longer-term product direction beyond summaries:

- cluster the same issue across meetings and show what changed, with citations;
- build reviewable project, decision, action, person, and glossary pages from accepted facts;
- suggest when an unfamiliar term may need an explanation; any external lookup is opt-in and must
  not send transcript context by default;
- maintain a local insight inbox for likely follow-ups, contradictions, missing owners, and aging
  decisions instead of interrupting the user;
- require confirmation before reminders, messages, tickets, web lookups, or any other external
  action, and keep a visible audit trail of the supporting meetings.

The current series digest, embeddings/RAG, accepted-record workflow, and safe identity candidates
are the foundation. PR #34 implements the first deterministic slice: it ranks missing action
ownership/dates, recurring risks, carried actions, and unresolved parking-lot topics from accepted
records only. Every suggestion links to its source; private notes, rejected claims, sentiment, and
employee-performance scores are excluded, and nothing is sent or changed automatically.

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

1. Finish the remaining 18 imports and the exact 50-hash integrity check.
2. Complete content labels and freeze a leakage-safe 12-15 meeting set by independent series.
3. Run local Qwen on that set, review references, and publish only aggregate quality metrics.
4. Run DeepSeek on the same set only when credentials and outbound consent are available; never silently substitute a provider.
5. Merge/review the existing `feat/` stack, including #31 before depending on RAG for cross-meeting output.
6. Validate suggestions, live facilitation, carry-forward, digest, insight inbox, and safe name candidates on held-out recordings.
7. Add confirmed outbound integrations only after the local workflow and audit trail are trustworthy.

The current reviewed data has only one confirmed `pure_status` meeting; most real standups include
deep dives. The release diversity gate must stay at three pure-status examples. The next 50-recording
batch should supply independent complete series rather than relabeling long discussions to make the
gate pass.
