#!/usr/bin/env node
/**
 * Generate Tauri Update Manifest from Local Files for GitHub Releases
 *
 * This script generates a Tauri-compatible update manifest JSON file
 * by reading local bundle files and creating GitHub Release URLs.
 *
 * Usage:
 *   node scripts/generate-update-manifest-github.js <version> [bundle-dir] [output-file] [notes]
 *
 * Example:
 *   node scripts/generate-update-manifest-github.js 0.1.2 frontend/src-tauri/target/release/bundle/updater latest.json "Release notes here"
 */

const fs = require('fs');
const path = require('path');

const [version, bundleDir = 'frontend/src-tauri/target/release/bundle/updater', outputFile = 'latest.json', notes = ''] = process.argv.slice(2);

if (!version) {
  console.error('Usage: node generate-update-manifest-github.js <version> [bundle-dir] [output-file] [notes]');
  console.error('Example: node generate-update-manifest-github.js 0.1.2 frontend/src-tauri/target/release/bundle/updater latest.json "Release notes"');
  process.exit(1);
}

// Detect system architecture for macOS builds
function detectMacOSArchitecture(bundleDir) {
  // Check if bundle directory path contains architecture hints
  if (bundleDir.includes('aarch64') || bundleDir.includes('arm64')) {
    return 'darwin-aarch64';
  }
  if (bundleDir.includes('x86_64') || bundleDir.includes('x64')) {
    return 'darwin-x86_64';
  }

  // Try to detect from system architecture
  try {
    const os = require('os');
    const arch = os.arch();
    if (arch === 'arm64') {
      return 'darwin-aarch64';
    } else if (arch === 'x64') {
      return 'darwin-x86_64';
    }
  } catch (e) {
    // Fallback if detection fails
  }

  // Default fallback - will be overridden by filename detection if possible
  return null;
}

// Remove 'v' prefix from version if present
const versionClean = version.replace(/^v/, '');
const versionDir = `v${versionClean}`;
const pubDate = new Date().toISOString();

console.log(`Generating manifest for version ${versionClean}...`);
console.log(`GitHub Repository: bykof/conversationaly`);
console.log(`Bundle Directory: ${bundleDir}`);
console.log('');

// Check if bundle directory exists
if (!fs.existsSync(bundleDir)) {
  console.error(`Error: Bundle directory not found: ${bundleDir}`);
  console.error('Make sure you\'ve built the release first: pnpm tauri:build');
  process.exit(1);
}

const platforms = {};

// Read all files in the bundle directory
const files = fs.readdirSync(bundleDir);

// Filter to only bundle files (not directories or signature files)
const bundleFiles = files.filter(filename => {
  const filePath = path.join(bundleDir, filename);
  const stats = fs.statSync(filePath);
  // Only process files (not directories) and skip signature files
  return stats.isFile() && !filename.endsWith('.sig') && (
    filename.endsWith('.tar.gz') ||
    filename.endsWith('.zip') ||
    filename.endsWith('.dmg') ||
    filename.endsWith('.exe') ||
    filename.endsWith('.msi') ||
    filename.endsWith('.AppImage') ||
    filename.endsWith('.deb')
  );
});

bundleFiles.forEach(filename => {
  const name = filename.toLowerCase();
  let platform = null;

  // Detect platform from filename
  // Check for tar.gz bundles first (most common for macOS/Linux)
  if (name.includes('darwin') || name.includes('macos') || name.includes('.dmg') || (name.includes('.app') && name.includes('.tar.gz'))) {
    if (name.includes('aarch64') || name.includes('arm64') || name.includes('m1') || name.includes('m2')) {
      platform = 'darwin-aarch64';
    } else if (name.includes('x86_64') || name.includes('x64') || name.includes('intel')) {
      platform = 'darwin-x86_64';
    } else {
      // Try to detect from system/bundle directory if filename doesn't specify
      const detectedArch = detectMacOSArchitecture(bundleDir);
      if (detectedArch) {
        platform = detectedArch;
      } else {
        // Default to aarch64 for modern macOS builds (most common)
        platform = 'darwin-aarch64';
      }
    }
  } else if (name.includes('windows') || name.includes('.exe') || name.includes('.msi') || (name.includes('.zip') && !name.includes('darwin') && !name.includes('macos'))) {
    platform = 'windows-x86_64';
  } else if (name.includes('linux') || name.includes('.appimage') || name.includes('.deb') || (name.includes('.tar.gz') && !name.includes('darwin') && !name.includes('macos'))) {
    platform = 'linux-x86_64';
  }

  if (platform && !platforms[platform]) {
    // Generate GitHub Release URL
    const githubUrl = `https://github.com/bykof/conversationaly/releases/download/${versionDir}/${filename}`;

    // Check if signature file exists (look for .sig file with same name)
    const sigFile = path.join(bundleDir, `${filename}.sig`);
    let signature = '';
    if (fs.existsSync(sigFile)) {
      try {
        signature = fs.readFileSync(sigFile, 'utf8').trim();
      } catch (error) {
        console.warn(`  ⚠ Failed to read signature file: ${error.message}`);
      }
    }

    platforms[platform] = {
      signature: signature,
      url: githubUrl
    };

    console.log(`✓ Found ${platform}: ${filename}`);
    if (signature) {
      console.log(`  ✓ Signature found: ${path.basename(sigFile)}`);
    } else {
      console.log(`  ⚠ No signature file found (expected: ${path.basename(sigFile)})`);
    }
  }
});

if (Object.keys(platforms).length === 0) {
  console.error('Error: No platform bundles found in the directory');
  console.error('Expected files with names containing: darwin, macos, windows, linux, .exe, .dmg, .app, .AppImage');
  process.exit(1);
}

const manifest = {
  version: versionClean,
  notes: notes || `Release ${versionClean}`,
  pub_date: pubDate,
  platforms
};

const outputPath = path.resolve(outputFile);
fs.writeFileSync(outputPath, JSON.stringify(manifest, null, 2));

console.log('');
console.log(`✓ Manifest generated: ${outputPath}`);
console.log(`\nNext steps:`);
console.log(`1. Create GitHub Release with tag: v${versionClean}`);
console.log(`   URL: https://github.com/bykof/conversationaly/releases/new?tag=v${versionClean}`);
console.log(`\n2. Upload this file to the release:`);
console.log(`   - File: ${outputFile}`);
console.log(`   - Name: latest.json (must be exact)`);
console.log(`\n3. Upload update bundles to the release:`);
Object.keys(platforms).forEach(platform => {
  const filename = platforms[platform].url.split('/').pop();
  console.log(`   - ${filename}`);
});
console.log(`\n4. Verify the manifest is accessible:`);
console.log(`   curl https://github.com/bykof/conversationaly/releases/latest/download/latest.json`);
