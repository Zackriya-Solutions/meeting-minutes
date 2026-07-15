# Memento quality gate

The gate computes aggregate quality metrics without writing meeting text into its report:

- transcription WER;
- diarization DER on a configurable time grid, with speaker-label permutation;
- retrieval Recall@k, MRR, and no-answer false-positive rate;
- summary success rate, p95 latency, fact coverage, unsupported-claim rate, and
  action-item F1.
- Standup V2 record coverage, unsupported decisions/actions, action F1, owner precision
  when an owner is shown, evidence validity, duplicate rate, provider success, and latency.

`pnpm quality:smoke` runs the committed synthetic fixture. It validates the evaluator and
regression wiring only; it is not evidence of model quality.

The release gate intentionally requires a private dataset that is not committed:

```bash
cd frontend
MEMENTO_QUALITY_DATASET=/absolute/path/release-eval.json pnpm quality:gate
```

The release dataset must contain at least 10 transcription samples, 10 diarization
samples, 30 Russian retrieval questions (at least 20 answerable and 5 unanswerable), and
10 summary runs, at least 12 manually typed standups, and at least 2 manually typed contrast
meetings. Filename hints do not count as those labels. Every successful summary run must include a manually reviewed `quality`
object with values from 0 to 1: `fact_coverage`, `unsupported_claim_rate`, and
`action_item_f1`. Use `fixtures/smoke.json` as the schema example. Generated reports
contain only counts and aggregate metrics; keep the source dataset under `evals/private/`
or outside the repository.

## Standup corpus workflow

Create a private 12-15-meeting annotation skeleton from the local Memento database:

```bash
cd frontend
python3 evals/prepare_standup_corpus.py \
  --db "$HOME/Library/Application Support/com.meetily.ai/meeting_minutes.sqlite" \
  --output evals/private/standup-corpus.json \
  --limit 15
```

The exporter ranks candidates using title, time, duration, and weak content markers. These
are candidate hints, not ground-truth meeting types. Review `series_id` before assigning a
split: an unreviewed series remains `UNASSIGNED` and deliberately fails the gate. A series
must appear in exactly one of `train`, `dev`, or `test`; chunks from the same series must
never be split across them.

After reviewing the ranked draft, freeze the exact meetings by repeating `--meeting-id` in
the desired order. An explicit ID that is missing or has no transcript fails loudly; `--limit`
is ignored for an explicit selection:

```bash
python3 evals/prepare_standup_corpus.py \
  --db "$HOME/Library/Application Support/com.meetily.ai/meeting_minutes.sqlite" \
  --output evals/private/standup-corpus-frozen.json \
  --meeting-id meeting-first \
  --meeting-id meeting-second
```

Manually set each sample's `meeting_type` to one of the top-level
`meeting_type_options`. `UNASSIGNED` deliberately counts as a protocol error. This makes
pure status rounds, status-plus-deep-dive meetings, planning/syncs, one-to-ones, and general
meetings distinguishable instead of treating a filename hint as ground truth.

For each sample, finish `reference_records` by adding facts the model missed as well as
confirming accepted records. Give every output record a `match_id` only when it is supported
by the reference fact of the same kind. Keep the transcript and record text in the private
dataset. The generated report contains aggregate counts and rates only.

Set `provider`, `schema_version`, and `prompt_version` from the actual run. Unknown values
are protocol errors. For a provider comparison, run each provider against the same frozen
reference set and keep the series split unchanged; the report includes per-provider success
and p95 latency alongside the shared record-quality metrics. The release gate requires at
least two dev and three held-out test meetings.

If the review workflow from the Standup V2 feature is present in the database, accepted and
rejected records seed the annotation automatically. That review cannot reveal facts omitted
by the model, so a human must still add missed reference records before changing
`review_state` and using the sample as release evidence.

Run the focused gate while iterating on providers or prompts:

```bash
MEMENTO_QUALITY_DATASET="$PWD/evals/private/standup-corpus.json" pnpm quality:standup
```

An untouched exporter skeleton is expected to fail: it has no frozen series split, no
completed Standup V2 run, and no finished reference labels yet. Never lower the thresholds
to make an unfinished corpus green.

For consistent summary review, first mark the reference transcript with atomic facts and
action items. `fact_coverage` is the covered reference-fact fraction;
`unsupported_claim_rate` is the unsupported output-claim fraction (zero when the summary
contains no unsupported claims); `action_item_f1` compares normalized owner/action/due-date
tuples. Reviewers should not score style or wording in these fields.

Thresholds live in `thresholds.release.json` and must be changed through a reviewed commit,
never implicitly after a failing run.
