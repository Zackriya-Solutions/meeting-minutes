# Legacy Backend Scripts Archive

This document previously described the scripts used to build and run the old
Python/FastAPI, Docker, and standalone whisper-server backend.

## Current Supported Development Flow

Meetily no longer uses these backend scripts for supported development or user
setup. The supported app is the Tauri desktop application under `frontend/`,
with Rust backend code in `frontend/src-tauri`.

Use these docs instead:

- [Top-level README](../README.md)
- [Building from Source](../docs/BUILDING.md)
- [Architecture](../docs/architecture.md)

## Archived Script Status

The legacy scripts in this directory are retained only for historical reference
and migration context. Do not use them for new installs, production
deployments, or contributor setup.

The old Docker deployment, FastAPI server, standalone whisper-server startup,
and unauthenticated/CORS behavior are unsupported and must not be treated as the
current Meetily backend.
