# Release Process for Meetily

This document outlines the release process for Meetily, including how to create releases with cross-platform whisper-server binaries.

## Table of Contents

- [Overview](#overview)
- [Automated Binary Builds](#automated-binary-builds)
- [Creating a Release](#creating-a-release)
- [Manual Binary Builds](#manual-binary-builds)
- [Release Checklist](#release-checklist)
- [Troubleshooting](#troubleshooting)

## Overview

Meetily releases include:
1. **Main Application Installers** - Platform-specific installers for the Tauri application
2. **Whisper-Server Binaries** - Pre-compiled whisper-server binaries for Windows, macOS, and Linux
3. **Documentation** - Updated documentation and release notes

## Automated Binary Builds

The project includes GitHub Actions workflows that automatically build whisper-server binaries for all supported platforms.

### Workflow: Build Whisper Server Binaries

**Location:** `.github/workflows/build-whisper-server.yml`

**Triggers:**
- **On Release Creation:** Automatically builds and attaches binaries when a new release is created
- **Manual Trigger:** Can be manually triggered from the Actions tab with custom model selection

**What it builds:**
- `whisper-server-windows-x64.zip` - Windows 64-bit binary with model
- `whisper-server-macos-universal.tar.gz` - macOS universal binary with model
- `whisper-server-linux-x64.tar.gz` - Linux 64-bit binary with model

Each package includes:
- Compiled whisper-server binary
- Selected Whisper model (default: small)
- Run script for easy startup
- Public web interface files

### How to Use the Workflow

#### Option 1: Automatic on Release

When you create a new release on GitHub, the workflow automatically:
1. Builds whisper-server for all platforms
2. Downloads the specified model (or default: small)
3. Packages everything into platform-specific archives
4. Attaches the archives to the release

#### Option 2: Manual Trigger

To manually build binaries:

1. Go to the [Actions tab](https://github.com/Zackriya-Solutions/meeting-minutes/actions)
2. Select "Build Whisper Server Binaries" workflow
3. Click "Run workflow"
4. Select the desired model size:
   - `tiny` - Fastest, ~75MB
   - `base` - Fast, ~142MB
   - `small` - Balanced (default), ~466MB
   - `medium` - Better accuracy, ~1.5GB
   - `large-v3` - Best accuracy, ~3GB
   - `large-v3-turbo` - Fast large model, ~1.6GB
5. Click "Run workflow"

The workflow will create artifacts that can be downloaded from the workflow run page.

## Creating a Release

### Prerequisites

- Maintainer access to the repository
- All changes merged to main branch
- Version number decided (following semantic versioning)
- Release notes prepared

### Step-by-Step Process

#### 1. Prepare the Release

```bash
# Ensure you're on the main branch
git checkout main
git pull origin main

# Update version numbers in:
# - frontend/src-tauri/tauri.conf.json
# - frontend/package.json
# - backend/app/main.py (if applicable)

# Commit version bump
git add .
git commit -m "chore: bump version to vX.X.X"
git push origin main
```

#### 2. Create a Git Tag

```bash
# Create and push tag
git tag -a vX.X.X -m "Release vX.X.X"
git push origin vX.X.X
```

#### 3. Create GitHub Release

**Option A: Using GitHub Web Interface**

1. Go to [Releases page](https://github.com/Zackriya-Solutions/meeting-minutes/releases)
2. Click "Draft a new release"
3. Select the tag you just created (vX.X.X)
4. Set release title: "Meetily vX.X.X"
5. Add release notes (see template below)
6. Check "Set as a pre-release" if applicable
7. Click "Publish release"

**Option B: Using GitHub CLI**

```bash
gh release create vX.X.X \
  --title "Meetily vX.X.X" \
  --notes-file RELEASE_NOTES.md \
  --draft  # Remove this to publish immediately
```

#### 4. Wait for Automated Builds

After creating the release:
1. The "Build Whisper Server Binaries" workflow will automatically trigger
2. Monitor progress in the [Actions tab](https://github.com/Zackriya-Solutions/meeting-minutes/actions)
3. Builds typically complete in 15-30 minutes
4. Binaries will be automatically attached to the release

#### 5. Build Main Application

Build the main Tauri application for each platform:

**macOS:**
```bash
cd frontend
pnpm install
pnpm run tauri:build
# Upload: src-tauri/target/release/bundle/dmg/Meetily_X.X.X_universal.dmg
```

**Windows:**
```bash
cd frontend
pnpm install
pnpm run tauri:build
# Upload: src-tauri/target/release/bundle/nsis/Meetily_X.X.X_x64-setup.exe
```

**Linux:**
```bash
cd frontend
pnpm install
pnpm run tauri:build
# Upload: src-tauri/target/release/bundle/appimage/Meetily_X.X.X_amd64.AppImage
```

#### 6. Upload Main Application Installers

Upload the built installers to the release:

**Using GitHub Web Interface:**
1. Go to the release page
2. Click "Edit release"
3. Drag and drop the installer files
4. Click "Update release"

**Using GitHub CLI:**
```bash
gh release upload vX.X.X \
  frontend/src-tauri/target/release/bundle/dmg/Meetily_X.X.X_universal.dmg \
  frontend/src-tauri/target/release/bundle/nsis/Meetily_X.X.X_x64-setup.exe \
  frontend/src-tauri/target/release/bundle/appimage/Meetily_X.X.X_amd64.AppImage
```

#### 7. Verify Release

Check that the release includes:
- [ ] Main application installers for all platforms
- [ ] Whisper-server binaries for all platforms
- [ ] Complete release notes
- [ ] Correct version number
- [ ] All assets are downloadable

#### 8. Announce Release

- Update the README.md with the latest version
- Post announcement on Discord
- Update website if applicable
- Share on social media

## Manual Binary Builds

If you need to build whisper-server binaries manually (e.g., for testing):

### Windows

```cmd
cd backend
build_whisper.cmd small
# Package will be created in backend/whisper-server-package/
```

### macOS

```bash
cd backend
./build_whisper.sh small
# Package will be created in backend/whisper-server-package/
```

### Linux

```bash
cd backend
./build_whisper.sh small
# Package will be created in backend/whisper-server-package/
```

Then create archives:

```bash
# Windows (from Git Bash or WSL)
cd backend
7z a whisper-server-windows-x64.zip whisper-server-package/*

# macOS/Linux
cd backend
tar -czf whisper-server-macos-universal.tar.gz whisper-server-package
# or
tar -czf whisper-server-linux-x64.tar.gz whisper-server-package
```

## Release Checklist

Use this checklist when creating a release:

### Pre-Release
- [ ] All planned features merged to main
- [ ] All tests passing
- [ ] Documentation updated
- [ ] Version numbers bumped
- [ ] CHANGELOG.md updated
- [ ] Release notes prepared

### Release Creation
- [ ] Git tag created and pushed
- [ ] GitHub release created
- [ ] Automated workflow triggered
- [ ] Main application built for all platforms
- [ ] All assets uploaded to release

### Post-Release
- [ ] Release verified (all assets present and downloadable)
- [ ] Installation tested on each platform
- [ ] README.md updated with latest version
- [ ] Announcement posted
- [ ] Homebrew formula updated (for macOS)

## Release Notes Template

```markdown
## Meetily vX.X.X

### 🎉 Highlights

- Brief description of major features or changes

### ✨ New Features

- Feature 1 description
- Feature 2 description

### 🐛 Bug Fixes

- Fix 1 description (#issue-number)
- Fix 2 description (#issue-number)

### 📚 Documentation

- Documentation improvement 1
- Documentation improvement 2

### 🔧 Technical Changes

- Technical change 1
- Technical change 2

### 📦 Downloads

#### Main Application
- **Windows:** `Meetily_X.X.X_x64-setup.exe`
- **macOS:** `Meetily_X.X.X_universal.dmg` or via Homebrew: `brew upgrade --cask meetily`
- **Linux:** `Meetily_X.X.X_amd64.AppImage`

#### Whisper Server Binaries (Optional)
For users who need to build from source or want standalone whisper-server:
- **Windows:** `whisper-server-windows-x64.zip`
- **macOS:** `whisper-server-macos-universal.tar.gz`
- **Linux:** `whisper-server-linux-x64.tar.gz`

Each package includes the whisper-server binary, a small model, and startup scripts.

### 📖 Documentation

- [Installation Guide](https://github.com/Zackriya-Solutions/meeting-minutes#installation)
- [Windows Setup Guide](https://github.com/Zackriya-Solutions/meeting-minutes/blob/main/docs/WINDOWS_SETUP.md)
- [Building from Source](https://github.com/Zackriya-Solutions/meeting-minutes/blob/main/docs/BUILDING.md)

### 🙏 Contributors

Thanks to all contributors who made this release possible!

### 📝 Notes

- Any important notes or breaking changes
- Migration instructions if applicable
```

## Troubleshooting

### Workflow Fails to Build

**Symptom:** GitHub Actions workflow fails during build

**Common Causes:**
1. **Submodule not initialized:** Ensure `submodules: recursive` is set in checkout action
2. **Missing dependencies:** Check that all required tools are installed in the workflow
3. **Model download fails:** Network issues or invalid model name

**Solution:**
- Check the workflow logs for specific error messages
- Verify the workflow YAML syntax
- Test the build locally before pushing

### Binaries Not Attached to Release

**Symptom:** Release is created but whisper-server binaries are missing

**Possible Causes:**
1. Workflow didn't trigger (check Actions tab)
2. Workflow is still running (wait for completion)
3. Workflow failed (check logs)

**Solution:**
- Manually trigger the workflow from Actions tab
- Or build and upload binaries manually

### Binary Doesn't Work on Target Platform

**Symptom:** Users report that the binary doesn't run

**Common Issues:**
1. **Windows:** Missing Visual C++ Redistributable
2. **macOS:** Binary not signed (Gatekeeper blocks it)
3. **Linux:** Missing shared libraries

**Solution:**
- Include troubleshooting steps in release notes
- Link to platform-specific setup guides
- Consider code signing for macOS binaries

### Model File Too Large

**Symptom:** Release assets are very large due to model files

**Solution:**
- Use smaller models by default (tiny or small)
- Provide instructions for downloading larger models separately
- Consider hosting models externally and providing download scripts

## Best Practices

1. **Test Before Release:** Always test the release on all platforms before publishing
2. **Semantic Versioning:** Follow semantic versioning (MAJOR.MINOR.PATCH)
3. **Clear Release Notes:** Provide clear, user-friendly release notes
4. **Backward Compatibility:** Maintain backward compatibility when possible
5. **Security:** Never include API keys or secrets in releases
6. **Documentation:** Keep documentation in sync with releases
7. **Communication:** Announce releases through appropriate channels

## Additional Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)
- [Tauri Release Documentation](https://tauri.app/v1/guides/distribution/)

## Questions?

If you have questions about the release process:
- Open an issue on GitHub
- Ask in the [Meetily Discord](https://discord.gg/crRymMQBFH)
- Contact the maintainers
