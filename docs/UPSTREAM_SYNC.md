# Keeping PulseTalk up to date

PulseTalk is maintained in a fork of Meetily.

```text
origin   https://github.com/Qblaauw/PulseTalk.git
upstream https://github.com/Zackriya-Solutions/meetily.git
```

Use `origin` for PulseTalk branches and releases. Use `upstream` only to fetch changes from the original project.

## Check for upstream changes

```bash
git fetch upstream --prune
git log --oneline main..upstream/main
```

The log shows commits in upstream that are not yet in the local PulseTalk branch.

## Bring upstream changes into PulseTalk

Start from a clean working tree and review the incoming commits before merging them.

```bash
git switch main
git fetch upstream --prune
git merge upstream/main
```

Resolve conflicts, then run the relevant Rust, frontend, and packaging checks. Pay special attention to branding, app identity, installer names, updater URLs, storage paths, and legal notices.

When the merge is ready:

```bash
git push origin main
```

## Develop PulseTalk features

Create feature branches from the current PulseTalk `main` branch:

```bash
git switch main
git pull --ff-only origin main
git switch -c feature/short-description
```

Push feature branches to the fork:

```bash
git push -u origin feature/short-description
```

## Important rules

- Do not push to `upstream`.
- Do not use `git reset --hard` to resolve sync conflicts.
- Review upstream branding changes before accepting them.
- Keep PulseTalk-specific work in separate commits where practical.
- Preserve compatibility for existing Meetily data paths until a migration plan is implemented.
- Re-run the branding audit after each upstream merge using the register in `docs/branding-migration-register.md`.

## Current branch relationship

At the time this guide was added, PulseTalk `main` contains the rebrand and local PulseTalk changes on top of upstream Meetily `main` at `v0.4.0`.
