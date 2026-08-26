# Changesets

Add one changeset for every user-visible change that should affect the next application version:

```bash
cd frontend
pnpm changeset
```

Use:

- `patch` for fixes
- `minor` for new functionality during pre-1.0 development
- `major` for stable breaking changes

The GitHub `Version Packages` workflow consumes these files, updates `frontend/package.json`, `CHANGELOG.md`, the root Cargo workspace version, and `Cargo.lock`, then opens a release PR.
