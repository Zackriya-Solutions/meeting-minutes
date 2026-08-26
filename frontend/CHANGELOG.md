# meet4specs

## 1.0.0

### Major Changes

- Promote the desktop application identity and release process to Meet4Specs 1.0.0.
- Rename all legacy package, crate, bundle, app data, Docker, workflow, analytics, and artifact references to Meet4Specs.
- Centralize version management across `frontend/package.json`, Tauri, Cargo, and frontend runtime constants.
- Add Changesets-powered version PR automation and tag-triggered release builds.
- Regenerate Windows/macOS application icons with Meet4Specs branding.
- Fix Windows clean build artifact paths and validation.

### Migration Notes

- Reinstall the application using the new Meet4Specs installer so shortcuts and icons are recreated.
- Update custom scripts, environment variables, local paths, and CI/release automation to use the new Meet4Specs package, crate, app data, and artifact names.
- If you rely on local files from a previous install, migrate or reselect those paths from the old application folders to the new Meet4Specs folders.

## 0.4.0

### Current release

- Centralized application versioning for Meet4Specs.
