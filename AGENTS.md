# Agent instructions

All coding agents, orchestration runtimes, and shell operators must follow
[the multi-agent operating model](docs/MULTI_AGENT_OPERATING_MODEL.md).

Before editing:

1. Inspect the branch, worktree, dirty state, and registered worktrees.
2. Resolve the stable task ID and target `integration/*` branch.
3. Work in one task-owned branch and worktree.
4. Declare expected files and collision-sensitive resources.

Do not edit `main` or the shared integration worktree as a task agent. Commit
coherent checkpoints, attach verification to the exact commit, and leave a
handoff that another runtime can resume without the original chat.

## Installer preflight

Before packaging any installer, the release coordinator must fetch and prune all
remotes, then inventory local and remote `feature/*`, `fix/*`, `hotfix/*`,
`chore/*`, and other task branches. Check task records, handoffs, pull requests,
commit history, and branch ancestry to identify completed work that is not yet
reachable from the release candidate.

Do not package from an arbitrary task branch. Merge approved completed work into
the target `integration/*` or `release/*` candidate, pull the combined candidate,
and run its verification contract. Every completed branch must be either:

1. reachable from the exact commit being packaged; or
2. listed in the release handoff with the reason and approval for deferral.

An unaccounted completed feature or bug fix blocks packaging. Record the branch,
commit SHA, dirty state, included and deferred work, checks, installer path, and
artifact checksum for the exact packaged commit.

Repository-specific architecture, commands, and product constraints remain in
[CLAUDE.md](CLAUDE.md). Human contribution rules remain in
[CONTRIBUTING.md](CONTRIBUTING.md).
