# Keeping PulseTalq up to date

PulseTalq is maintained in a fork of Meetily.

This guide covers upstream ingestion. Product and agent work follows the
[multi-agent operating model](MULTI_AGENT_OPERATING_MODEL.md). Upstream changes
must pass through their own integration branch before they reach `main`.

```text
origin   https://github.com/Qblaauw/PulseTalq.git
upstream https://github.com/Zackriya-Solutions/meetily.git
```

Use `origin` for PulseTalq branches and releases. Use `upstream` only to fetch changes from the original project.

## Check for upstream changes

```bash
git fetch upstream --prune
git log --oneline main..upstream/main
```

The log shows commits in upstream that are not yet in the local PulseTalq branch.

## Bring upstream changes into PulseTalq

Start from a clean working tree and review the incoming commits before merging
them. Create a dedicated integration branch so upstream changes can be reconciled
with PulseTalq identity, local features, and release configuration before
promotion.

```bash
git switch main
git pull --ff-only origin main
git fetch upstream --prune
git switch -c integration/upstream-YYYY-MM-DD
git merge upstream/main
```

Resolve conflicts, then run the relevant Rust, frontend, and packaging checks. Pay special attention to branding, app identity, installer names, updater URLs, storage paths, and legal notices.

When the integration branch is verified, open a PR to `main` or perform the
approved release promotion:

```bash
git push -u origin integration/upstream-YYYY-MM-DD
```

## Develop PulseTalq features

Create task branches and worktrees from the current target integration branch.
The integration coordinator creates and publishes that branch from `main` before
parallel work starts.

```bash
git fetch origin --prune
git worktree add .worktrees/TASK-ID-short-description \
  -b feature/TASK-ID-short-description \
  origin/integration/pulsetalq-next
```

Push feature branches to the fork:

```bash
git push -u origin feature/short-description
```

## Important rules

- Do not push to `upstream`.
- Do not develop directly on `main` or in the shared integration worktree.
- Do not use `git reset --hard` to resolve sync conflicts.
- Review upstream branding changes before accepting them.
- Keep PulseTalq-specific work in separate commits where practical.
- Preserve compatibility for existing Meetily data paths until a migration plan is implemented.
- Re-run the branding audit after each upstream merge using the register in `docs/branding-migration-register.md`.

## Current branch relationship

At the time this guide was added, PulseTalq `main` contains the rebrand and local PulseTalq changes on top of upstream Meetily `main` at `v0.4.0`.
