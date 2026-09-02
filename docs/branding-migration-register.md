# PulseTalq branding migration register

This register records the remaining Meetily, Zackriya, and related branding references found in the repository. It groups work so each cluster can be updated and verified together.

## Audit scope

- Searched tracked source, documentation, scripts, CI workflows, configuration, and asset filenames under this repository.
- Matched `Meetily`, `meetily`, `meetly`, `Zackriya`, `zackriya`, repository URLs, app identifiers, product names, storage paths, updater names, logo references, icon references, and obvious screenshot/GIF branding assets.
- Excluded `.git`, `target`, dependency folders, and lockfiles from the content search. Git metadata still contains the old upstream remote and should be handled separately if the repository is being transferred.
- The exact standalone spelling `meetly` produced no additional matches. Most legacy product references use `Meetily`.

## Target decisions to confirm before editing

| Existing value | Intended PulseTalq value | Decision needed |
|---|---|---|
| `Meetily` / `meetily` | `PulseTalq` / `pulsetalq` | Product display name and lowercase technical slug |
| `com.meetily.ai` | A new PulseTalq bundle identifier | Must be chosen before packaging and release work |
| `Zackriya Solutions` | PulseTalq owner or approved legal attribution | Confirm copyright, support, and privacy-policy ownership |
| `Zackriya-Solutions/meeting-minutes` | PulseTalq repository | Confirm final GitHub organization/repository |
| `https://meetily.ai` and Zackriya URLs | PulseTalq website/support URLs | Confirm canonical URLs |
| `MEETILY_*` environment/secrets | `PULSETALQ_*` or compatibility names | Decide whether old secret names remain temporarily supported |
| `Meetily` application-data folders | `PulseTalq` folders or a migration path | Preserve existing user data when upgrading |

## Cluster A: user-facing product copy and public documentation

Update these together so the product name, positioning, links, and legal wording stay consistent.

| Priority | Location | Current reference | Required update |
|---|---|---|---|
| P0 | `README.md` | Meetily title, sections, badges, release links, website, Discord, Reddit, PRO/Enterprise copy, clone URL, screenshots, Star History URL | Rewrite as PulseTalq documentation. Replace or remove Meetily commercial/community copy that does not describe PulseTalq. Update all links, image alt text, and repository references. |
| P0 | `PRIVACY_POLICY.md` | Meetily policy title/body/version, Zackriya GitHub issues, Zackriya contact form | Replace product and owner references, confirm the support channel, and revalidate policy scope/version. |
| P0 | `CONTRIBUTING.md` | Meetily welcome text and Zackriya repository URL | Update project name and contribution/upstream instructions. |
| P0 | `BLUETOOTH_PLAYBACK_NOTICE.md` | Eight Meetily references, including title-level wording, FAQ text, and version scope | Replace product name and confirm the linked support/document location. |
| P1 | `docs/architecture.md` | Meetily product description | Update product name and any architecture claims that are no longer current after the PulseTalq fork. |
| P1 | `docs/BUILDING.md` | Meetily title/body and `Meetily_<version>.AppImage` output example | Update title, commands, output examples, and artifact names. |
| P1 | `docs/building_in_linux.md` | Meetily title/body and `Meetily_<version>.AppImage` output example | Apply the same Linux build-documentation changes as `docs/BUILDING.md`. |
| P1 | `docs/GPU_ACCELERATION.md` | Meetily product references | Update product name and verify that the supported acceleration instructions match PulseTalq. |
| P1 | `CLAUDE.md` | Meetily product description and Meetily macOS/Windows model paths | Update project guidance and path examples, while documenting any legacy-path compatibility. |
| P1 | `frontend/README.md` | Meetily title/features, clone URL, app description, and local transcription text | Rewrite as PulseTalq frontend documentation and update repository instructions. |
| P1 | `frontend/API.md` | Older Meetily development-flow/release references | Update product name and clarify whether this legacy API document remains supported. |
| P1 | `backend/API_DOCUMENTATION.md` | Meetily backend release references | Update product name or mark this as a legacy backend document. |
| P1 | `backend/README.md` | Meetily backend references | Update product name, current support status, and links. |
| P1 | `backend/SCRIPTS_DOCUMENTATION.md` | Meetily references | Update product name and any paths/commands documented here. |
| P1 | `frontend/src-tauri/templates/README.md` | Meetily template references | Update product name and user-data paths if those paths are changing. |
| P2 | `.github/workflows/WORKFLOWS_OVERVIEW.md` | Meetily artifact names and `MEETILY_RSA_PUBLIC_KEY` | Update workflow examples, artifact naming, and secret names together with CI changes. |
| P2 | `docs/plans/2026-09-01-windows-dictation-v1-design.md` | Meetily dependency/compatibility references | Update the design record only where it describes current PulseTalq ownership or product naming. Preserve historical context where it explains the fork. |
| P2 | `project/decisions.md` | Meetily local models and upstream compatibility | Clarify which statements are historical and replace current PulseTalq references. |

## Cluster B: app identity, metadata, and visible UI copy

These changes affect what users see in the app and what the operating system calls it.

| Priority | Location | Current reference | Required update |
|---|---|---|---|
| P0 | `frontend/package.json` | Package name `meetily` | Change package/app slug after deciding the npm and build naming convention. |
| P0 | `frontend/src-tauri/tauri.conf.json` | `productName: meetily`, identifier `com.meetily.ai`, window title `meetily`, updater endpoint on Zackriya GitHub | Replace display name, bundle identifier, window title, and updater endpoint. Treat identifier and updater changes as release/migration work, not a text-only rename. |
| P0 | Windows installation surfaces generated from `frontend/src-tauri/tauri.conf.json` | The app is currently installed and discoverable as `Meetily`/`meetily`, including the Start menu, Settings > Installed apps, window title, shortcuts, and likely executable/package labels | Verify every Windows-facing label after rebuilding. Rename these surfaces to PulseTalq, and document the migration behavior for existing Meetily installations. |
| P0 | `frontend/src/app/metadata.ts` | Page title `Meetily` | Change title and description to PulseTalq copy. |
| P0 | `frontend/src/app/metadata.tsx` | Duplicate page title `Meetily` | Change title and description, then decide whether this duplicate metadata file should be removed or consolidated. |
| P0 | `frontend/src/components/About.tsx` | Visible About copy: `What makes Meetily different`, `Chat with the Zackriya team`, `Built by Zackriya Solutions` | Replace with the approved PulseTalq copy. The supplied example appears to correspond directly to this component. Update privacy-policy destination and any product version copy at the same time. |
| P0 | `frontend/src/components/Info.tsx` | `About Meetily` tooltip and dialog title | Change to `About PulseTalq`. |
| P0 | `frontend/src/components/Logo.tsx` | Generic `Logo` component loads legacy-looking `/logo.png` and `/logo-collapsed.png` | Replace assets or confirm they are PulseTalq assets, and improve accessible alt text if needed. |
| P1 | `frontend/src/components/AnalyticsConsentSwitch.tsx` | Privacy-policy link points to Zackriya `meeting-minutes` GitHub | Point to the approved PulseTalq policy URL or local policy route. |
| P1 | `frontend/src/components/BluetoothPlaybackWarning.tsx` | Link points to `your-org/meetily` | Replace with the canonical PulseTalq documentation URL. |
| P1 | `frontend/src/components/onboarding/OnboardingFlow.tsx` | Meetily reference | Update onboarding product copy. |
| P1 | `frontend/src/components/onboarding/steps/WelcomeStep.tsx` | Meetily reference | Update welcome copy. |
| P1 | `frontend/src/components/onboarding/steps/SetupOverviewStep.tsx` | Meetily reference | Update setup copy. |
| P1 | `frontend/src/components/onboarding/steps/PermissionsStep.tsx` | Meetily reference | Update permission copy. |
| P1 | `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx` | Meetily reference | Update download/model copy. |
| P1 | `frontend/src/contexts/OnboardingContext.tsx` | Meetily reference | Update context-level user-facing or diagnostic text. |
| P1 | `frontend/src/components/PreferenceSettings.tsx` | Meetily reference | Update visible settings copy. |
| P1 | `frontend/src/components/PermissionWarning.tsx` | Meetily reference | Update visible warning copy. |
| P1 | `frontend/src/components/Sidebar/index.tsx` | Meetily reference and logo placement | Update visible text and coordinate with the new logo assets. |
| P1 | `frontend/src/components/TranscriptView.tsx` | Meetily reference | Update visible or diagnostic copy. |
| P1 | `frontend/src/components/VirtualizedTranscriptView.tsx` | Meetily reference | Update visible or diagnostic copy. |
| P1 | `frontend/src/components/AnalyticsConsentSwitch.tsx` | Product-independent analytics text plus old policy URL | Keep the supplied privacy/analytics wording if approved, but update the link and any product name. |

## Cluster C: visual assets, logos, icons, screenshots, and filenames

These require visual review, not only string replacement. Images may contain baked-in Meetily text that a text search cannot detect.

| Priority | Location | Asset/reference | Required update |
|---|---|---|---|
| P0 | `frontend/public/logo.png` | Main web logo | Replace with approved PulseTalq logo and keep the filename only if no code change is desired. |
| P0 | `frontend/public/logo-collapsed.png` | Collapsed/sidebar logo | Replace with approved PulseTalq mark and verify light/dark/background contrast. |
| P0 | `frontend/public/icon_128x128.png`, `frontend/public/icon_32x32@2x.png` | Public app icons | Replace with PulseTalq icons and verify favicon/app usage. |
| P0 | `frontend/src-tauri/icons/icon*.png`, `icon.ico`, `icon.icns` | Tauri icon family | Replace the entire generated icon set from one approved source image. Do not update only one size. |
| P0 | `frontend/src-tauri/icons/app_icon.ico`, `app_icon.icns` | Alternate app icon family referenced by Tauri config | Decide whether these are still needed, then replace or remove consistently. |
| P0 | `frontend/src-tauri/icons/Square*Logo.png`, `StoreLogo.png`, `128x128*`, `32x32.png` | Windows/store icon family | Replace with PulseTalq artwork and verify installer/store presentation. |
| P1 | `frontend/src/app/favicon.ico` | Browser/app favicon | Replace with PulseTalq favicon. |
| P1 | `docs/PulseTalq-6.png` | README hero image, renamed from `Meetily-6.png` | Replace or visually approve the asset and inspect it for baked-in branding. |
| P1 | `docs/pulsetalq_demo.gif` | README demo, renamed from `meetily_demo.gif` | Replace or visually approve the asset and inspect all frames for legacy branding. |
| P1 | `docs/pulsetalq-export.gif` | README export demo, renamed from `meetily-export.gif` | Replace or visually approve the asset and inspect all frames for legacy branding. |
| P1 | `docs/logo1.png`, `docs/logo2.png`, `docs/logo3.png` | Documentation logo assets | Determine whether these are source logo variants, screenshots, or obsolete files. Replace, rename, or mark historical. |
| P1 | `docs/home.png`, `docs/local.png`, `docs/custom.png`, `docs/settings.png`, `docs/summary.png`, `docs/transcription.png`, `docs/audio.png`, `docs/editor.png`, `docs/editor1.png`, `docs/device_selection.png`, `docs/pv2.0.png`, `docs/pv2.1.png`, `docs/1.png` through `docs/8.png`, `docs/demo_small.gif`, `docs/HighLevel.jpg`, `docs/Diagram-High level architecture diagram.jpg` | Documentation screenshots/diagrams | Perform a visual sweep for old logos, app titles, URLs, and UI copy. Update or label each asset as PulseTalq. Text search cannot verify these files. |
| P2 | `frontend/public/next.svg`, `frontend/public/vercel.svg`, `frontend/public/window.svg`, `frontend/public/file.svg`, `frontend/public/globe.svg` | Default scaffold assets | Confirm whether these are used. Remove unused third-party starter assets or replace them if they appear in PulseTalq UI. |

## Cluster D: release, updater, CI, and distribution identifiers

Update this cluster as one release change. A partial rename can publish artifacts that the app cannot discover or install.

| Priority | Location | Current reference | Required update |
|---|---|---|---|
| P0 | `.github/workflows/release.yml` | Meetily release title, `meetily` asset prefix, `s3://meetily-updates`, GitHub release flow | Update release title, artifact prefix, update storage, and repository URLs. |
| P0 | `.github/workflows/build.yml` | Default app name `meetily`, `MEETILY_RSA_PUBLIC_KEY` | Update input/default naming and signing secret strategy. |
| P0 | `.github/workflows/build-devtest.yml` | `MEETILY_RSA_PUBLIC_KEY`, `meetily-devtest-*` artifacts | Update secret and artifact naming. |
| P0 | `.github/workflows/build-macos.yml` | `MEETILY_RSA_PUBLIC_KEY`, `meetily-macos-*` artifacts | Update secret and artifact naming. |
| P0 | `.github/workflows/build-linux.yml` | `MEETILY_RSA_PUBLIC_KEY`, `meetily-linux-*` artifacts | Update secret and artifact naming. |
| P0 | `.github/workflows/build-windows.yml` | `MEETILY_RSA_PUBLIC_KEY`, `meetily-windows-*` artifacts | Update secret and artifact naming. |
| P0 | `.github/workflows/build-test.yml` | `meetily-test` artifact prefix | Update test artifact naming. |
| P0 | Windows installer artifacts produced by the Tauri NSIS/MSI bundle | Installer display name, Start menu shortcut, uninstall entry, installer filename, and executable/package labels may still use `Meetily` even when the release file is generically named `x64-setup.exe` | Verify the generated NSIS and MSI packages on Windows, then rename all user-facing installer surfaces to PulseTalq. |
| P1 | `scripts/generate-update-manifest-github.js` | Zackriya repository, release/download URLs, release instructions | Replace repository and artifact URL construction; verify manifest schema and signatures. |
| P1 | `scripts/test-update-locally.js` | Meetily test-server label and Zackriya updater endpoint | Update test labels and endpoint examples. |
| P1 | `frontend/src/lib/analytics.ts`, `frontend/src-tauri/src/analytics/` | Analytics event identity/version context where applicable | Verify product/application identifiers sent to analytics and update consent documentation. |
| P1 | `frontend/build.ps1`, `frontend/build.bat`, `frontend/build_backup.bat`, `frontend/build-gpu.ps1`, `frontend/build-gpu.bat`, `frontend/build-gpu.sh`, `frontend/dev-gpu.ps1`, `frontend/dev-gpu.bat`, `frontend/dev-gpu.sh` | Meetily build labels and/or signing-key filenames | Update display labels, output names, and signing-key references. Keep `build_backup.bat` only if it is still used. |
| P1 | `backend/build-docker.ps1`, `backend/build-docker.sh`, `backend/docker-compose.yml` | `meetily-backend` image/project names | Rename image/project identifiers or explicitly mark the backend as legacy. Update compose service names and documentation together. |
| P1 | `backend/run-docker.ps1`, `backend/run-docker.sh`, `backend/start_with_output.ps1`, `backend/setup-db.ps1`, `backend/setup-db.sh`, `backend/install_dependancies_for_windows.ps1` | Meetily service labels, image names, or paths | Update operational labels and any persisted service names. |
| P2 | `frontend/src-tauri/Cargo.toml`, `frontend/src-tauri/build.rs`, `frontend/src-tauri/build/ffmpeg.rs` | Build-time Meetily references | Update crate/build metadata and generated-resource labels where they affect released binaries. |
| P1 | `frontend/src-tauri/build.rs` | Build warning says `Building Meetily` | Update the build diagnostic to PulseTalq. |
| P1 | `frontend/src-tauri/build/ffmpeg.rs` | FFmpeg downloads hosted in `Zackriya-Solutions/ffmpeg-binaries` | Confirm whether the binary source remains authorized and stable. Move to the approved PulseTalq dependency location if required. |
| P1 | `frontend/src-tauri/Cargo.toml` | Crate name `meetily` and Zackriya `meeting-minutes` repository URL | Update crate/package metadata and repository URL, while checking package identifiers used by release tooling. |
| P1 | `frontend/src-tauri/NOTIFICATION_TESTING.md` | Meetily notification test reference | Update test instructions and expected notification titles. |

## Cluster E: runtime storage, migrations, environment variables, and compatibility

This is the highest-risk technical cluster. Decide the migration policy before changing names.

| Priority | Location | Current reference | Required update |
|---|---|---|---|
| P0 | `frontend/src-tauri/src/summary/templates/loader.rs`, `frontend/src-tauri/src/summary/templates/mod.rs` | `Meetily` app-data directories for templates | Add a PulseTalq path and a deliberate legacy-path migration/read fallback if existing users must retain templates. |
| P0 | `frontend/src-tauri/src/summary/summary_engine/model_manager.rs` | `.join("Meetily")` model path | Migrate or support the existing model directory before renaming. |
| P0 | `frontend/src-tauri/src/audio/recording_preferences.rs` | `meetily-recordings` default folders on Windows, macOS, and Linux | Decide whether new recordings use `pulsetalq-recordings`, and preserve/discover the old folder for existing users. |
| P0 | `frontend/src-tauri/src/notifications/commands.rs`, `frontend/src-tauri/src/notifications/settings.rs`, `frontend/src-tauri/src/notifications/types.rs` | `Meetily` notification titles and lowercase `meetily` settings path | Update visible notification titles and runtime identifiers, while preserving existing notification state if needed. |
| P0 | `frontend/src/components/DatabaseImport/HomebrewDatabaseDetector.tsx` | Legacy Homebrew path containing `meetily` | Keep as a legacy import path, add PulseTalq path, and label the compatibility behavior clearly. |
| P0 | `frontend/src/components/DatabaseImport/LegacyDatabaseImport.tsx` | Meetily legacy database import references | Preserve the old identifier for discovery, but update user-facing text to PulseTalq and document migration behavior. |
| P0 | `frontend/src/services/indexedDBService.ts` | Meetily storage/database reference | Decide whether the storage key/database name is user data and requires migration. |
| P1 | `scripts/inject_transcript.py` | Meetily database description, OS data paths, CLI messages | Support PulseTalq paths plus legacy Meetily lookup, and update user-facing messages. |
| P1 | `frontend/src-tauri/src/summary/summary_engine/sidecar.rs` | `MEETILY_LLAMA_HELPER` environment variable and error text | Introduce the PulseTalq variable, optionally support the old variable during transition, and update errors. |
| P1 | `frontend/src-tauri/src/audio/decoder.rs` | `.meetily_decode_` temporary-file prefix | Rename new temp files, but assess whether old temp files need cleanup compatibility. |
| P1 | `frontend/src-tauri/src/audio/capture/core_audio.rs` | `meetily-audio-tap` Core Audio identifier | Rename only with care. Check whether the identifier is persisted, permission-related, or externally referenced. |
| P2 | `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`, `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs`, `frontend/src-tauri/src/console_utils/console_utils.rs`, `frontend/src-tauri/src/lib_old_complex.rs` | Internal Meetily labels/logging or legacy code references | Update active diagnostics. Mark truly historical code as legacy or remove it in a separate cleanup. |
| P1 | `frontend/src-tauri/src/tray.rs` | System-tray tooltip `Meetily` | Change the tray tooltip to PulseTalq. |
| P1 | `frontend/src-tauri/src/audio/capture/core_audio.rs` | Core Audio tap identifier `meetily-audio-tap` | Rename only with care. Check whether the identifier is persisted, permission-related, or externally referenced. |

## Cluster F: ownership, upstream repository, and legal provenance

These references may be intentionally retained for attribution or third-party provenance. Do not remove them until ownership is confirmed.

| Priority | Location | Current reference | Required update |
|---|---|---|---|
| P0 | `LICENSE.md` | Copyright `Zackriya Solutions` | Confirm whether this copyright must remain, be amended, or be supplemented with PulseTalq ownership. Preserve legally required notices. |
| P0 | `.gitmodules` | Zackriya whisper.cpp URL | Confirm whether this is an upstream dependency that should remain, move, or be vendored under a new repository. |
| P1 | `.git/config`, `.git/FETCH_HEAD` | Zackriya/Meetily upstream remotes and branch metadata | Not part of published docs, but update repository remotes/metadata if this working copy is being fully transferred. Avoid rewriting history without an explicit request. |
| P1 | `CONTRIBUTING.md`, `README.md`, `PRIVACY_POLICY.md`, `scripts/generate-update-manifest-github.js` | Zackriya GitHub and website links | Replace only after the PulseTalq canonical organization, support channel, and policy location are known. |

## Suggested update order

1. Confirm the target identity table above, especially bundle ID, repository, updater host, legal owner, and storage migration policy.
2. Replace approved logo/icon assets and inspect screenshots/GIFs. This makes visual review possible while copy changes land.
3. Update user-facing UI and public docs together, including the About panel copy supplied in the request.
4. Update runtime compatibility paths and environment variables with migration tests.
5. Update CI, release artifacts, updater manifests, signing secrets, and distribution names as one release change.
6. Re-run the audit with case-insensitive searches for `meetily`, `meetly`, `zackriya`, `meeting-minutes`, `com.meetily`, `MEETILY_`, and `meetily_`. Then perform a visual asset sweep.

## Completion checklist

- [ ] No unintended visible Meetily/Zackriya references remain.
- [ ] Old user data, templates, models, database imports, and settings have a tested migration or explicit compatibility fallback.
- [ ] Bundle ID, app-data directory policy, signing secrets, artifact names, update manifest URLs, and release storage agree.
- [ ] Legal attribution and third-party notices are correct.
- [ ] README, privacy policy, contribution guide, build guides, backend docs, and workflow docs point to PulseTalq locations.
- [ ] All logos, icons, favicons, screenshots, diagrams, and GIF frames have passed visual review.
