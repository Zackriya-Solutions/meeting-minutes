# Auto-update (Tauri updater → SberCloud OBS)

The desktop app updates itself with `tauri-plugin-updater`, checking a `latest.json`
manifest we host on SberCloud OBS (S3-compatible). Same hosting and credentials as
GigaTool's updater; the manifest format is Tauri's, not electron-updater's.

## What signs the update

Tauri verifies a **minisign signature** over the downloaded payload against the public
key compiled into the app (`plugins.updater.pubkey` in
[frontend/src-tauri/tauri.conf.json](../frontend/src-tauri/tauri.conf.json)). This is
independent of Apple notarization — an update with a valid Developer ID signature but a
wrong minisign signature is rejected, and vice versa. Ship both.

| | |
| --- | --- |
| Key ID | `5A2DE95D19CB005E` |
| Private key | `~/.memento/updater/memento-updater.key` (mode 600, **not** in the repo) |
| Public key | `~/.memento/updater/memento-updater.key.pub` |
| Password | none |

**Back the private key up somewhere durable.** Lose it and no already-installed client
can ever be updated again — every user has to reinstall by hand, because the replacement
public key only reaches them inside a new build.

This is a fork-specific key, not upstream's. Users running a build installed from
`Zackriya-Solutions/meeting-minutes` will **not** auto-migrate to our builds: their app
polls upstream's GitHub endpoint and trusts upstream's key. They must install our build
once manually; after that they follow our channel.

### Regenerating the keypair

```bash
frontend/node_modules/.bin/tauri signer generate -w ~/.memento/updater/memento-updater.key -p ""
base64 < ~/.memento/updater/memento-updater.key.pub   # -> plugins.updater.pubkey
```

Then update `pubkey` in `tauri.conf.json` and the key ID in the table above. Every client
must install a build carrying the new key before it can receive key-signed updates.

## Endpoint and object layout

The app polls exactly one URL:

```
https://obs.ru-moscow-1.hc.sbercloud.ru/d-ssdev-crowd/function_descriptions/memento/latest.json
```

`scripts/publish-update-obs.py` writes this layout:

```
function_descriptions/memento/
├── latest.json                                    # Cache-Control: no-cache
└── v<version>/
    ├── darwin-aarch64/Memento.app.tar.gz{,.sig}   # Cache-Control: immutable
    ├── darwin-aarch64/Memento_<v>_aarch64.dmg     # direct download, not in the manifest
    ├── darwin-x86_64/Memento.app.tar.gz{,.sig}
    └── windows-x86_64/Memento_<v>_x64-setup.exe{,.sig}
```

Payloads are namespaced by version **and** platform because Tauri names the macOS payload
`Memento.app.tar.gz` — no version, no arch. A flat layout would have arm64 and x86_64
overwrite each other, and an immutable `Cache-Control` on a reused key would pin the first
release's binary forever.

One manifest covers every platform, so publishing a second platform for the same version
**merges** into the manifest already on OBS rather than replacing it. arm64, x86_64 and
Windows can therefore be built on different machines, at different times, in any order. A
different version starts a fresh manifest. `--no-merge` overrides this.

## In-app flow

| Trigger | Code |
| --- | --- |
| 2s after app mount, throttled to once per 24h | [useUpdateCheck.ts](../frontend/src/hooks/useUpdateCheck.ts) |
| Tray → "Check for Updates" (forced check) | [tray.rs:205](../frontend/src-tauri/src/tray.rs) → `check-updates-from-tray` event |
| Settings → About → "Check for Updates" (forced) | [About.tsx](../frontend/src/components/About.tsx) |

An available update raises a notification; opening it shows
[UpdateDialog](../frontend/src/components/UpdateDialog.tsx), which downloads with a
progress bar, installs, and relaunches. The dialog refuses to close mid-download (ESC and
outside clicks are blocked). Updates are always optional — there is no forced-update path.

A failed check is swallowed on startup so a dead endpoint can never block app launch;
manual checks surface the error.

## Release checklist

### 1. Bump the version

Tauri compares versions with **semver**, so `0.4.1` is fine and `0.4.0.1` is not — a
four-component version fails to parse and the client silently gets no update. Bump both:

- `frontend/src-tauri/tauri.conf.json` → `version`
- `frontend/package.json` → `version`

> Note: `.github/workflows/release.yml` auto-appends a fourth component (`0.4.0.1`) when a
> tag already exists. That scheme is incompatible with the updater — publish releases whose
> version is plain semver.

### 2. Build signed

```bash
frontend/build-mac-signed.sh          # arm64
frontend/build-mac-x86-signed.sh      # Intel
```

Both scripts pick up `~/.memento/updater/memento-updater.key` automatically and flip
`createUpdaterArtifacts` on, so the build emits `Memento.app.tar.gz` + `.sig` next to the
`.app`. Without a key they print a warning and build a normal (unpublishable) DMG instead
of failing. Look for this line near the end:

```
==> Updater artifact:  .../Memento.app.tar.gz  + .sig
```

The payload is tarred from the already signed, notarized and stapled `.app`, so the update
a user installs is Gatekeeper-clean.

### 3. Publish

```bash
export S3_ACCESS_KEY_ID=...  S3_SECRET_ACCESS_KEY=...   # or put them in frontend/.env.signing

scripts/publish-update-obs.py --dry-run                 # inspect the manifest first
scripts/publish-update-obs.py --notes "Что нового в этой версии"

# Intel build lives under its own target triple:
scripts/publish-update-obs.py --from target/x86_64-apple-darwin/release/bundle
```

The script uploads payloads first, HEADs each one anonymously to prove `public-read` took
effect, and only then writes `latest.json` — a client can never see a manifest pointing at
an object that is missing or 403. Afterwards it reads the manifest back over HTTPS and
confirms the served version.

Useful flags: `--version`, `--target darwin-x86_64` (force the platform key),
`--notes-file`, `--no-installers` (skip the DMG), `--no-merge`, `--no-verify`, `--no-public`.

Settings come from the environment, `.env`, or `frontend/.env.signing` (both gitignored):

| Var | Default |
| --- | --- |
| `OBS_ENDPOINT` | `https://obs.ru-moscow-1.hc.sbercloud.ru` |
| `OBS_REGION` | `ru-moscow-1` |
| `OBS_BUCKET` | `d-ssdev-crowd` |
| `OBS_PREFIX` | `function_descriptions/memento` |

`OBS_PREFIX` must stay in sync with `plugins.updater.endpoints[0]`; the script warns when
the URL it publishes to isn't the one the app polls.

### 4. Verify against a real client

```bash
curl -s https://obs.ru-moscow-1.hc.sbercloud.ru/d-ssdev-crowd/function_descriptions/memento/latest.json | jq
```

Then launch an older installed build and use tray → **Check for Updates** (bypasses the
24h throttle). Watch for `[UpdateDialog]` progress lines in the app console.

To rehearse without touching OBS, serve a manifest locally with
`node scripts/test-update-locally.js` (port 8080) and point
`plugins.updater.endpoints` at `http://localhost:8080/latest.json` in a scratch build.

## OBS quirks

`scripts/publish-update-obs.py` papers over the same three that GigaTool hit:

1. **Virtual-hosted addressing is mandatory for PUT.** Path-style reads work (the updater
   fetches over a path-style URL), but uploading with `addressing_style=path` returns
   `NoSuchBucket`.
2. **boto3 ≥ 1.36 streaming SHA256.** OBS can't parse the chunked checksum trailer and
   returns `XAmzContentSHA256Mismatch`; we set `request_checksum_calculation=when_required`.
3. **Objects are private by default.** The updater reads anonymously, so everything is
   uploaded `public-read`.

## Troubleshooting

| Symptom | Cause |
| --- | --- |
| `incorrect updater private key password: Device not configured` | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is unset. It must be exported *even when empty* — otherwise the CLI tries to read a password from the tty. The build scripts do this for you. |
| `failed to decode base64 secret key: Invalid symbol 46` | A file **path** was put in `TAURI_SIGNING_PRIVATE_KEY`. That variable takes the key *contents*; use `TAURI_SIGNING_PRIVATE_KEY_PATH` for a path. |
| Build aborts asking for a signing key | `createUpdaterArtifacts` is `true` in `tauri.conf.json` and no key was found. Run a build script (they handle it) or export the key. |
| Client reports "no update available" while `latest.json` looks right | Manifest version is ≤ the installed version, or isn't valid semver. |
| Manual "Check for Updates" errors on every client | Nothing published yet — OBS answers `403` for a key that doesn't exist, and the plugin treats that as a failed check. Startup checks swallow it; only manual checks surface it. Goes away with the first publish. |
| Only some platforms error | The manifest has no entry for that `platforms` key. A Windows client checking a mac-only manifest fails with "platform windows-x86_64 was not found". Publish every platform you ship for a given version. |
| Client 403s while downloading | Payload uploaded without `public-read`. Re-run publish (its pre-flight HEAD check catches this). |
| Update installs but the app won't launch | Payload was built from an unsigned/un-notarized `.app`. Rebuild with `build-mac-signed.sh`. |

## CI

The GitHub workflows (`build-macos.yml`, `build-windows.yml`, `build.yml`, …) already pass
`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from repository secrets,
but those secrets still hold nothing for this fork's key. To build updater artifacts in CI,
set `TAURI_SIGNING_PRIVATE_KEY` to the **contents** of `memento-updater.key` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to an empty string. Releases are published from a
developer machine today; CI publishing to OBS is not wired up.
