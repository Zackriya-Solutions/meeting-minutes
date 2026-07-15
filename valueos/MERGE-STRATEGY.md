# Merge Strategy & Conflict-Avoidance Policy

This fork tracks an actively-developed upstream (`Zackriya-Solutions/meetily`). Every
edit we make to an upstream file is a potential merge conflict later. This policy keeps
those conflicts to a minimum. Follow it for all VA work in this repo.

---

## Rule 1 — New logic goes in new files

New capabilities, helpers, config, and documentation go in **new files**, preferably
under `valueos/`. Never grow a feature inside an existing upstream source file when a
new file would do. A file that only we created can never conflict with upstream.

## Rule 2 — When an upstream file MUST be touched, mark it

Sometimes an integration genuinely requires editing an upstream file (e.g. registering
a Tauri command, adding a hook call). When that happens:

- Keep the change **minimal and localized** — the fewest lines possible, in one place.
- **Wrap and comment** the change with a `// VALUEOS:` marker so it is greppable:

  ```rust
  // VALUEOS: forward finished transcript into the funnel pipeline
  valueos::transcript::forward(&transcript).await?;
  // VALUEOS: end
  ```

  Use the language-appropriate comment syntax (`// VALUEOS:` for Rust/TS,
  `{/* VALUEOS: */}` in JSX, `<!-- VALUEOS: -->` in HTML/Markdown, `# VALUEOS:` in
  shell/TOML).

- This lets us audit every upstream deviation with a single search:

  ```bash
  git grep -n "VALUEOS:"
  ```

## Rule 3 — Prefer defined integration hook points

Rather than scattering small edits across many upstream files, funnel our integration
through a **small number of well-defined hook points**. For example: one call site that
hands a completed transcript to VA code living under `valueos/`, instead of ten edits
sprinkled through the audio and summary pipeline. Fewer, deliberate touch-points mean
fewer conflicts and a clearer boundary between upstream and VA code.

## Rule 4 — Protected files: do not edit unless unavoidable

Do **not** edit the following unless there is genuinely no alternative — and if you
must, document the exception in the table below:

- The root **`CLAUDE.md`** (upstream-authored).
- Build scripts: **`clean_run.sh`**, **`clean_build.sh`**,
  **`clean_run_windows.bat`**, **`clean_build_windows.bat`**.
- Package / dependency manifests: **`package.json`**, **`pnpm-lock.yaml`**,
  **`Cargo.toml`**, **`Cargo.lock`**, and `tauri.conf.json`.

These files change frequently upstream and/or are central to the build, so edits here
are the most conflict-prone and the most disruptive when they break.

### Documented exceptions

If Rule 4 must be broken, record it here so the next engineer understands why the
protected file diverges from upstream.

| Date | File | Why the edit was unavoidable | Marker | Author |
|------|------|------------------------------|--------|--------|
| _(none yet)_ | | | | |

---

## Quick checklist before you commit

- [ ] Could this have lived in a new file under `valueos/`? If yes, move it there.
- [ ] Every upstream-file edit is minimal and wrapped in `// VALUEOS:` markers.
- [ ] `git grep -n "VALUEOS:"` shows every deviation and nothing unexpected.
- [ ] No protected file (Rule 4) touched — or the exception is logged above.
- [ ] Feature branch has recently merged `main` so upstream drift is small.
