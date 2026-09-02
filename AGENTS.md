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

Repository-specific architecture, commands, and product constraints remain in
[CLAUDE.md](CLAUDE.md). Human contribution rules remain in
[CONTRIBUTING.md](CONTRIBUTING.md).
