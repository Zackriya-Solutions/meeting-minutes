# MeetilyHeb Agent Workflow

## Branching And Worktrees

- Do feature work in a separate git worktree, not directly in the main checkout.
- Keep `main` reserved for merged, releasable code.
- Use short, descriptive branch names such as `codex/gemini-transcription` or `codex/local-updater`.
- Before editing, check the worktree status and avoid overwriting unrelated user changes.
- Do not use destructive git commands such as `git reset --hard` or `git checkout --` unless explicitly requested.

Suggested flow:

```bash
git fetch --prune
git worktree add ../Meetily-heb-<topic> -b codex/<topic> main
cd ../Meetily-heb-<topic>
```

## Pull Requests And Merging

- Put all non-trivial changes through a PR.
- Run the relevant checks before opening or merging the PR.
- Merge PRs into `main`; do not leave the primary checkout on long-lived feature branches.
- After merging, update the main checkout:

```bash
cd /Users/elad.moshe/my-code/Meetily-heb
git checkout main
git pull --ff-only
```

## Keeping The Local App Synced

After a PR is merged into `main`, run the local updater from the main checkout so the installed binary always matches merged code:

```bash
cd /Users/elad.moshe/my-code/Meetily-heb
scripts/update-local-macos.sh --launch
```

Use `--force-quit` only when the app refuses to quit normally:

```bash
scripts/update-local-macos.sh --launch --force-quit
```

For periodic automatic updates on this machine, install the LaunchAgent only after `main` is clean and has an upstream configured:

```bash
scripts/install-local-updater-launchagent.sh
```

The updater intentionally refuses to `--pull` over uncommitted changes.

## Build Checks

Use the bundled Node/pnpm path when needed:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/Users/elad.moshe/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin:/Users/elad.moshe/.cache/codex-runtimes/codex-primary-runtime/dependencies/bin:$PATH"
```

Common verification commands:

```bash
cargo check -p meetilyheb
cd frontend && pnpm build
```

