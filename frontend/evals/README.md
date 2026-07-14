# Memento quality gate

The gate computes aggregate quality metrics without writing meeting text into its report:

- transcription WER;
- diarization DER on a configurable time grid, with speaker-label permutation;
- retrieval Recall@k, MRR, and no-answer false-positive rate;
- summary success rate, p95 latency, fact coverage, unsupported-claim rate, and
  action-item F1.

`pnpm quality:smoke` runs the committed synthetic fixture. It validates the evaluator and
regression wiring only; it is not evidence of model quality.

The release gate intentionally requires a private dataset that is not committed:

```bash
cd frontend
MEMENTO_QUALITY_DATASET=/absolute/path/release-eval.json pnpm quality:gate
```

The release dataset must contain at least 10 transcription samples, 10 diarization
samples, 30 Russian retrieval questions (at least 20 answerable and 5 unanswerable), and
10 summary runs. Every successful summary run must include a manually reviewed `quality`
object with values from 0 to 1: `fact_coverage`, `unsupported_claim_rate`, and
`action_item_f1`. Use `fixtures/smoke.json` as the schema example. Generated reports
contain only counts and aggregate metrics; keep the source dataset under `evals/private/`
or outside the repository.

For consistent summary review, first mark the reference transcript with atomic facts and
action items. `fact_coverage` is the covered reference-fact fraction;
`unsupported_claim_rate` is the unsupported output-claim fraction (zero when the summary
contains no unsupported claims); `action_item_f1` compares normalized owner/action/due-date
tuples. Reviewers should not score style or wording in these fields.

Thresholds live in `thresholds.release.json` and must be changed through a reviewed commit,
never implicitly after a failing run.
