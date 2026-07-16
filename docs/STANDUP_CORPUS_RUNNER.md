# Standup corpus runner

The runner executes the ordinary Standup V2 pipeline sequentially for an explicit set of local
meeting IDs. It is intended for reproducible provider and prompt evaluation, not automatic data
discovery. A completed Standup V2 result is skipped only when its provider, model, template ID,
and template fingerprint match the requested run; otherwise it is regenerated.

```bash
cd frontend
MEETILY_STANDUP_CORPUS_IDS="meeting-id-1,meeting-id-2" \
MEETILY_STANDUP_CORPUS_PROVIDER="builtin-ai" \
MEETILY_STANDUP_CORPUS_MODEL="qwen3.5:4b" \
MEETILY_STANDUP_CORPUS_LANGUAGE="ru-RU" \
MEETILY_STANDUP_CORPUS_REPORT="$PWD/evals/private/qwen-run.json" \
pnpm run tauri:dev
```

By default, a provenance-matching Standup V2 result is skipped and any other completed summary is
left untouched with an `overwrite_required` report item. Set
`MEETILY_STANDUP_CORPUS_OVERWRITE=true` only for an intentional destructive rerun. The same
operation is available to local callers as `start_standup_corpus_run`, but that command is confined
to the IDs, overwrite permission, and exact report path declared by these environment variables.

Fully quit Meetily (including its menu-bar/tray process) before starting the command. The desktop
app enforces a single instance; if an older instance is still resident, the new corpus-mode process
exits before its startup runner can execute.

The JSON report contains meeting IDs, titles, status, latency, chunk count, extracted-record count,
and sanitized provider-error categories. Raw provider errors are never copied into the report,
because an upstream error may echo request text. Every item also records its provider, model, and template fingerprint,
so a resumed report cannot silently attribute an old result to a new run. It deliberately contains
neither transcripts nor generated facts. Keep it under `evals/private/` anyway because titles may
contain personal data.

The report starts with `state: running` before the first meeting and is replaced atomically after
every result. A database, model, provider failure, or unexpected per-meeting panic is recorded
against that meeting and does not abort the rest of the corpus. Panic details stay in the local
application log rather than the report. The final checkpoint has `state: completed` and a non-null
`completed_at`. Restarting without overwrite skips only provenance-matching completed Standup V2
results and reports their stored record counts.

For a fair comparison:

1. Freeze 12–15 reviewed meetings and their series-level train/dev/test split.
2. Run every provider against exactly those IDs, language, template, and schema version.
3. Export the hypotheses after each provider run before overwriting them with the next provider.
4. Evaluate evidence validity, unsupported decisions/actions, record coverage, action F1, owner
   precision, duplicates, success rate, and latency. Never compare prose style by eye alone.

Cloud providers still pass through the app's outbound-consent and credential checks. A DeepSeek
run fails visibly when neither a configured key nor a managed gateway is available; it never
silently falls back to another provider.
