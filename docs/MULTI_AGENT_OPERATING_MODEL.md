# Multi-agent operating model

This document defines how Techtraders, SALT, Pulse/ProductOps, Claude Cowork,
Claude Code, Codex, T3 Code, human contributors, and shell automation work on
the same product without losing changes between sessions, branches, or tools.

The repository owns product truth. A chat, agent session, local worktree, build
folder, or task tracker may point to that truth, but none replaces it.

## The model

```text
main                                      stable, releasable product
└── integration/<release-or-theme>        shared integration branch
    ├── agent/<task-id>-<topic>           agent-owned task branch
    ├── feature/<task-id>-<topic>         product feature branch
    ├── fix/<task-id>-<topic>             normal defect branch
    ├── hotfix/<task-id>-<topic>          urgent production repair
    └── docs|chore|refactor|test/<topic>  bounded supporting work
```

Each active task branch has one editable worktree and one owner at a time. The
integration branch also has a worktree, but only the integration coordinator
edits or merges there. Other agents treat that worktree as read-only product
state.

Worktrees isolate changes. Commits make work visible. Merges combine work.
Creating more worktrees does not synchronize their uncommitted files.

## Shared rules

1. `main` is stable and releasable. No agent develops directly on it.
2. `integration/*` is the current combined product candidate. Task branches
   merge here after focused verification.
3. Each mutable task gets its own branch and worktree. One branch represents
   one reviewable outcome.
4. Agents commit coherent checkpoints before another task depends on them.
5. Agents fetch the integration branch before starting and before handing off.
6. Only the integration coordinator merges task branches into `integration/*`.
7. Only a validated integration candidate may move to `main`.
8. Uncommitted work is never treated as shared product state.
9. A build claim names its branch, exact commit SHA, dirty state, target, and
   verification result.
10. No runtime claims completion from chat history alone. The durable evidence
    must exist in the repository or its linked Git provider record.

## Authority across Techtraders, SALT, Pulse, and agent runtimes

These responsibilities apply even when the named system is unavailable. A
different runtime may perform the work, but it must write the same artifacts and
respect the same authority boundaries.

| Participant | Responsibility | Durable output |
|---|---|---|
| Techtraders | Portfolio priority, product ownership, commercial constraints, environment authority, and release approval | Approved intent, product decision, release authorization, or linked business record |
| SALT | Reusable behavioral rules, quality standards, identity constraints, and decision discipline | Versioned policy, skill, rule set, or repository pointer |
| Pulse/ProductOps | Lifecycle routing, stable task identity, dependencies, ownership, status, evidence links, and surfaced gaps | `project/` records, plans, decisions, task ledger entries, and handoffs |
| Claude Cowork | Collaborative discovery, intent shaping, specification review, research, and stakeholder-facing artifacts | Reviewed `intent`, specification, decision, or handoff artifact committed to the repository |
| Claude Code, Codex, and T3 Code | Planning, implementation, focused review, verification, and task delivery | Task branch commits, tests, build receipts, review findings, and PR records |
| Humans | Judgment calls, policy exceptions, conflict decisions, release approval, and production accountability | Review decision, approval record, merge, or release record |
| Git and GitHub | Shared history, branch ancestry, review boundary, and delivery record | Commits, branches, pull requests, tags, checks, and releases |
| Bare shell and automation | Repeatable mechanics that do not require product judgment | Command result, generated receipt, check result, or deployment record |

Runtime names are host references, not task identities. A task keeps the same
stable ID when it moves between Cowork, Claude Code, Codex, T3 Code, or a human.

## Canonical artifact chain

Every stage reads the previous stage's accepted artifact and writes the next one.

```text
intent
  -> specification and product decisions
  -> implementation plan and task graph
  -> task branch commits and focused checks
  -> integration candidate and combined verification
  -> pull request and review evidence
  -> main, release artifact, and deployment record
  -> incident or learning that starts the next intent
```

Chat may explain or review an artifact. It is not the artifact.

## Task identity and ownership

Before an agent edits code, the task record must contain:

| Field | Meaning |
|---|---|
| Stable task ID | Repository-owned identifier that survives runtime and session changes |
| Intent or issue | Why the work exists and what outcome is expected |
| Target integration branch | The product candidate that will receive the work |
| Base ref and SHA | The exact integration state used to create the task branch |
| Task branch | The branch that owns the reviewable outcome |
| Owner and host reference | Current agent or human owner plus runtime session reference when available |
| Planned files and exclusive resources | Expected edit scope and collision-sensitive files, services, ports, or build targets |
| Dependencies | Tasks or artifacts that must land first |
| Verification contract | Tests, builds, screenshots, measurements, or manual checks required |
| Delivery evidence | Commit SHA, PR, integration merge SHA, and final verification receipt |

The local worktree path and process IDs may stay machine-local. Branches, SHAs,
task IDs, file ownership, and evidence pointers must survive the session.

## Starting work

The coordinator follows this sequence:

1. Confirm the intent, acceptance criteria, and target integration branch.
2. Create or activate the stable task record.
3. Inspect active tasks for file or resource collisions.
4. Fetch the latest remote integration branch.
5. Create the task branch from that exact integration SHA.
6. Create one worktree for the task branch.
7. Record the branch, base SHA, owner, planned files, dependencies, and checks.
8. Run the baseline checks before editing.

Example topology:

```text
.worktrees/
├── integration-pulsetalq-0.5/     integration coordinator only
├── B-2026-09-02-001-audio-fix/    one task owner
├── T42-dictation-icons/            one task owner
└── T43-release-docs/               one task owner
```

## Working in parallel

Agents may run concurrently when their declared files and exclusive resources do
not collide. If two tasks need the same file, schema, release configuration,
installer identity, or generated artifact, the coordinator serializes that work
or assigns one task as the owner.

During execution, each agent:

1. Stays inside its assigned worktree.
2. Changes only its declared scope unless the task record is updated.
3. Commits coherent checkpoints with the task ID.
4. Links changed files, checks, and commit SHAs to the task record.
5. Fetches integration before declaring the task ready.
6. Reports conflicts or newly discovered dependencies instead of silently
   changing another task's contract.

Agents inspect shared progress through Git and the task ledger:

```bash
git worktree list
git fetch origin --prune
git log --oneline --left-right integration/<name>...<task-branch>
git diff --stat integration/<name>...<task-branch>
```

## Handoff contract

An agent handoff is complete only when another operator can continue without the
original chat. It records:

- stable task ID and current status;
- branch, base SHA, head SHA, and dirty state;
- intent, specification, and plan links;
- files changed and known collision points;
- checks run with pass, fail, or unrun status;
- build artifact path and checksum when applicable;
- open decisions, blockers, and the exact next safe action;
- target integration branch and merge readiness.

"The branch is clean" is insufficient. The handoff must say which branch,
worktree, and SHA were checked.

## Integration

The integration coordinator owns the shared integration worktree. The
coordinator does not absorb an agent's uncommitted directory. It integrates
committed task outcomes.

For each ready task:

1. Confirm the task branch head and clean state.
2. Confirm focused verification against that exact head.
3. Check ancestry and changes since the recorded base SHA.
4. Reconcile conflicts against current product decisions and specifications.
5. Merge the complete task branch into `integration/*`.
6. Run affected checks on the combined integration tree.
7. Record the integration merge SHA in the task record.
8. Tell active agents to fetch the new integration head when their work depends
   on it.

Integration conflicts are product decisions when both sides encode valid but
different behavior. An agent must not resolve those by choosing whichever text
applies cleanly.

## Release promotion

The integration candidate may move to `main` only when the exact integration SHA
has current evidence for the affected product areas. A PulseTalq desktop release
normally checks:

- frontend type and behavior checks;
- Rust checks and relevant tests;
- Windows, macOS, or Linux packaging for the release targets;
- product name, bundle identifier, icons, installer labels, and updater URLs;
- migrations and compatibility with existing local user data;
- privacy, licensing, provenance, and release notes;
- artifact names, checksums, signing state, and commit provenance.

The release record names any skipped target or unrun check. "Build successful"
never means every release gate passed.

## Branch types

PulseTalq uses a small set of branch classes across all runtimes:

| Branch | Use |
|---|---|
| `main` | Stable release source |
| `integration/<release-or-theme>` | Shared candidate that receives completed task branches |
| `agent/<task-id>-<topic>` | Runtime-neutral agent task when another semantic type is not clearer |
| `feature/<task-id>-<topic>` | New user or product capability |
| `fix/<task-id>-<topic>` | Normal defect repair |
| `hotfix/<task-id>-<topic>` | Urgent repair based on the released `main` state |
| `chore/<topic>` | Maintenance, dependencies, CI, or repository operations |
| `docs/<topic>` | Documentation-only outcome |
| `refactor/<topic>` | Internal restructuring with no intended behavior change |
| `test/<topic>` | Test or verification infrastructure |
| `release/<version>` | Short-lived release preparation when required |

Do not create one permanent branch containing all bugs or all features. The
prefix classifies the work; the branch still represents one outcome.

## Runtime entry rules

Every runtime uses the same start and stop contract.

At session start:

1. Read this document and the runtime-specific repository pointer.
2. Inspect the current branch, worktree, dirty state, and registered worktrees.
3. Resolve or create the stable task ID.
4. Confirm the target integration branch and base SHA.
5. Claim the task before editing.

At session end:

1. Commit or explicitly preserve unfinished work on the task branch.
2. Update the task record and verification evidence.
3. Record the exact next action and any authority it requires.
4. Release or transfer ownership. Do not leave an invisible active claim.

Claude Cowork sessions that only shape or review intent do not need an
implementation worktree. They still commit or link the accepted artifact so a
coding runtime can start from it.

## Recovery and stale work

If an agent or host stops unexpectedly, its branch and worktree remain evidence.
The coordinator inspects the task record, branch head, dirty state, and last
owner heartbeat before reassigning it. Reassignment changes the owner reference;
it does not create a second task for the same outcome.

Never delete a task branch or worktree until its commits are reachable from the
intended integration or release branch and the retirement action has been
reviewed.

## Non-negotiable failures

The following states block integration:

- work exists only in chat or as uncommitted files in another worktree;
- branch or build provenance is missing;
- two agents own the same files or exclusive resource without coordination;
- a task branch was based on stale product state and has not reconciled it;
- a feature was tested alone but not on the combined integration tree;
- the integration branch contains unresolved product identity or release config;
- an agent reports completion without a commit and verification record;
- a release artifact cannot be tied to an exact clean or declared-dirty SHA.

## Adoption for PulseTalq

The existing `feature/windows-dictation-v1` work and current `main` changes must
meet on a temporary `integration/*` branch. The integration pass must preserve
the full dictation commit stack, Hub UI work, PulseTalq identity, current icons,
and later fixes. The combined Windows build, not either source branch alone,
becomes the release candidate.

Future Techtraders projects can reuse this model by changing product-specific
release checks while keeping the same task identity, branch, worktree, handoff,
integration, and promotion rules.

**Created:** 2026-09-02 . **Last opened:** 2026-09-02 . **Last edited:** 2026-09-02 . **Status:** stable . **Owner:** Q. Blaauw
