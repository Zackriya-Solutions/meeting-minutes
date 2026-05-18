# Legacy Whisper Server API Archive

This document previously described the standalone whisper-server HTTP API used
by older Meetily development flows.

## Current Supported Integration

The supported Meetily app no longer requires a manually started whisper-server
HTTP service. The Next.js UI communicates with the Rust/Tauri core through
Tauri commands and events, and local transcription is handled inside the
desktop application.

Use these docs for current development:

- [Frontend README](README.md)
- [Building from Source](../docs/BUILDING.md)
- [Architecture](../docs/architecture.md)

## Archived Status

The old whisper-server API is retained only as historical context for older
branches or migration research. It is not a supported public API for current
Meetily releases.
