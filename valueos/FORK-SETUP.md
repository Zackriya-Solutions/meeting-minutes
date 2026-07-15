# Fork Setup & Maintenance Runbook

This is a reproducible runbook documenting exactly how the `valueos-agent` fork was set
up and how to maintain it. Any VA engineer should be able to read this and either
replicate the setup or understand the state of an existing clone.

- **Upstream**: `Zackriya-Solutions/meetily` (open source, MIT).
- **Our fork / origin**: `value-accelerator/valueos-agent`.
- **Model**: consume-updates-only. We pull upstream changes in; we do **not** open PRs
  against upstream.

---

## 1. How the fork was created

The fork was created once, via the GitHub UI:

1. Sign in to GitHub as (or with access to) the `value-accelerator` organization.
2. Go to <https://github.com/Zackriya-Solutions/meetily>.
3. Click **Fork**, and set the owner to **value-accelerator** and the repository name
   to **valueos-agent**.
4. Uncheck "Copy the `main` branch only" if you want all upstream branches; for our
   purposes copying only `main` is fine.

This produced `https://github.com/value-accelerator/valueos-agent`.

## 2. Cloning the fork

```bash
git clone https://github.com/value-accelerator/valueos-agent.git
cd valueos-agent
```

## 3. Adding the upstream remote

By default a clone only knows about `origin` (our fork). Add `upstream` so we can pull
in Meetily updates:

```bash
git remote add upstream https://github.com/Zackriya-Solutions/meetily.git
```

## 4. Disabling accidental pushes to upstream

We never push to upstream. Disable the push URL so an accidental `git push upstream`
fails fast instead of attempting to write to Meetily:

```bash
git remote set-url --push upstream DISABLE
```

## 5. Verifying the remote configuration

```bash
git remote -v
```

Expected output:

```
origin    https://github.com/value-accelerator/valueos-agent.git (fetch)
origin    https://github.com/value-accelerator/valueos-agent.git (push)
upstream  https://github.com/Zackriya-Solutions/meetily.git (fetch)
upstream  DISABLE (push)
```

Note the key details:

- `origin` has both **fetch** and **push** (we work here).
- `upstream` has a **fetch** URL (we pull updates)…
- …but its **push** URL is `DISABLE`, so `git push upstream` errors out.

## 6. Update workflow (pulling upstream into our fork)

Do this whenever you want to bring in the latest Meetily changes. Always start from a
clean `main`:

```bash
git checkout main
git fetch upstream
git merge upstream/main
git push origin main
```

Then propagate the updated `main` into any active feature branches so they stay current
and conflicts surface early:

```bash
git checkout feature/<name>
git merge main
# resolve any conflicts, then:
git push origin feature/<name>
```

## 7. Branch strategy

- **`main`** — mirrors upstream `main` plus any merged, stable VA work. Treat it as the
  integration branch; keep it always buildable.
- **`feature/*`** — one branch per new capability (e.g. `feature/valueos-docs`,
  `feature/transcript-forwarding`). Branch off `main`, do the work, merge back into
  `main` when stable. Regularly merge `main` back into long-lived feature branches to
  minimize drift from upstream.

## 8. Important: consume-updates-only

We **do not** contribute changes back to `Zackriya-Solutions/meetily`. This fork exists
solely to consume upstream updates and layer VA-specific work on top. The disabled
upstream push URL (step 4) enforces this at the tooling level.
