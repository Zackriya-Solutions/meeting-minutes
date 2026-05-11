# Legacy Backend Archive

This directory contains the archived Python/FastAPI, Docker, and standalone
whisper-server backend implementation from older Meetily releases.

## Current Supported Architecture

Meetily no longer uses this backend as the supported application path. The
current app is a self-contained Tauri desktop application:

- Next.js provides the desktop UI from `frontend/src`.
- Rust/Tauri provides the local backend and native integration from
  `frontend/src-tauri`.
- Local transcription, meeting storage, and summary workflows are handled by
  the bundled desktop app rather than a separate FastAPI service.

Use these docs for supported setup and development:

- [Top-level README](../README.md)
- [Building from Source](../docs/BUILDING.md)
- [Architecture](../docs/architecture.md)

## Status of This Directory

The files under `backend/` are retained only for historical reference and
legacy migration context. They should not be used for new installs, production
deployments, security assessments of the supported app, or contributor setup.

The old FastAPI service, Docker compose flow, standalone whisper-server flow,
and related scripts are unsupported. The old unauthenticated FastAPI/CORS
behavior must not be treated as a supported production API.
