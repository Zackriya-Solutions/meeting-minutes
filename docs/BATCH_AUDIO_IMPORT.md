# Resumable batch audio import

The desktop app can import a folder recursively either from the Import Audio dialog or from a
trusted local Tauri caller using `start_batch_import_folder_command`. Imports are sequential and
deduplicated by the SHA-256 hash stored in each meeting's `metadata.json`, so rerunning the same
folder resumes safely.

For corpus work, the development app also accepts startup environment variables:

```bash
cd frontend
MEETILY_BATCH_IMPORT_FOLDER=/absolute/path/to/audio \
MEETILY_BATCH_IMPORT_PROVIDER=gigaam \
MEETILY_BATCH_IMPORT_REPORT=/absolute/path/to/import-report.json \
pnpm run tauri:dev
```

Optional variables are `MEETILY_BATCH_IMPORT_LANGUAGE` and `MEETILY_BATCH_IMPORT_MODEL`. The
selected provider model must already be installed. The report contains imported, skipped, failed,
and cancelled items. A failure or panic in one file is recorded and does not abort the remaining
queue.

Do not point automated corpus runs at folders containing recordings that have not been approved
for local processing. Audio, transcripts, metadata, and reports remain local, but they may contain
personal data.
