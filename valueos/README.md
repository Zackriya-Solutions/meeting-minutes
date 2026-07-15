# ValueOS Agent

**ValueOS Agent** is Value Accelerator's (VA) internal build of
[Meetily](https://github.com/Zackriya-Solutions/meetily), an open-source
(MIT-licensed) privacy-first AI meeting assistant. This repository is a **fork** of
that upstream project, rebranded internally as "ValueOS Agent".

## What it is

Meetily is a Tauri 2.x desktop application (Rust core + Next.js 14 / React 18
frontend, all under `/frontend`) that captures, transcribes, and summarizes meetings
entirely on local infrastructure — no meeting audio or transcript leaves the machine
unless the user chooses to send it.

## Why we forked it

We use ValueOS Agent as a **local meeting agent**: it records and generates
transcripts on-device, and in future VA-built extensions it will **forward those
transcripts into our funnels and on to ValueOS**. The local-first, privacy-preserving
foundation of Meetily is exactly the base we want; our work sits on top of it rather
than replacing it.

## The golden rule

> **All VA work lives under `valueos/` or in clearly-marked new files. We minimize
> edits to upstream files so that merges from upstream stay clean.**

Concretely:

- New logic, docs, and configuration go in **new files** — preferably under
  `valueos/`.
- Upstream source files (everything under `/frontend`, the root `CLAUDE.md`, build
  scripts, and package/Cargo manifests) are touched **only when unavoidable**, and any
  such edit is kept minimal and marked with a `// VALUEOS:` comment so it is greppable.
- We **consume** upstream updates; we do **not** contribute back upstream.

See the companion docs in this folder:

- [FORK-SETUP.md](FORK-SETUP.md) — how this fork was created and how to keep it in
  sync with upstream.
- [MERGE-STRATEGY.md](MERGE-STRATEGY.md) — the conflict-avoidance policy that keeps
  upstream merges painless.

## Layout

```
valueos-agent/
├── frontend/          # UPSTREAM — Tauri app (Rust src-tauri + Next.js). Minimize edits.
├── backend/           # UPSTREAM — archived Python/FastAPI. Unsupported. Ignore.
├── CLAUDE.md          # UPSTREAM — do not edit.
└── valueos/           # VA — all our docs and (later) code lives here.
    ├── README.md
    ├── FORK-SETUP.md
    └── MERGE-STRATEGY.md
```
