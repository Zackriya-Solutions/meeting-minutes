# Legacy Backend API Archive

This document previously described the Python/FastAPI API used by older
Meetily backend releases.

## Current Supported API Surface

Meetily no longer supports the standalone FastAPI backend as the active
application API. The supported application is the Tauri desktop app, where the
Next.js UI communicates with the Rust core through Tauri commands and events.

Use these docs for the supported architecture and build flow:

- [Top-level README](../README.md)
- [Building from Source](../docs/BUILDING.md)
- [Architecture](../docs/architecture.md)

## Security and Support Notice

The archived FastAPI API was unauthenticated and had development-oriented CORS
behavior. It must not be treated as a supported production API or used as the
basis for new deployments.

This file is retained only to explain why older references may exist in the
repository and to support migration research for users coming from legacy
installations.
