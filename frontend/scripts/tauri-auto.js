#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features
 */

const { execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

// Get the command (dev or build)
const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build]');
  process.exit(1);
}

// Detect GPU feature
let feature = '';

// Check for environment variable override first
if (process.env.TAURI_GPU_FEATURE) {
  feature = process.env.TAURI_GPU_FEATURE;
  console.log(`🔧 Using forced GPU feature from environment: ${feature}`);
} else {
  try {
    const result = execSync('node scripts/auto-detect-gpu.js', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'inherit']
    });
    feature = result.trim();
  } catch (err) {
    // If detection fails, continue with no features
  }
}

console.log(''); // Empty line for spacing

// Platform-specific environment variables
const platform = os.platform();
const env = { ...process.env };

if (platform === 'linux' && feature === 'cuda') {
  console.log('🐧 Linux/CUDA detected: Setting CMAKE flags for NVIDIA GPU');
  env.CMAKE_CUDA_ARCHITECTURES = '75';
  env.CMAKE_CUDA_STANDARD = '17';
  env.CMAKE_POSITION_INDEPENDENT_CODE = 'ON';
}

// Updater artifacts are signed with a private key that only the release workflow
// has, so a local build bundles the .app/DMG fine and then fails on signing.
// Skip them unless the key is present. Same override as clean_build.sh.
let configArg = '';
if (command === 'build' && !process.env.TAURI_SIGNING_PRIVATE_KEY) {
  // Passed as a file rather than inline JSON: execSync goes through a shell, and
  // single-quoted JSON is not quoted on Windows cmd.exe. A double-quoted path
  // works on both, and tolerates spaces in the repo path.
  const overridePath = path.join(os.tmpdir(), 'meetily-tauri-no-updater.json');
  fs.writeFileSync(overridePath, JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
  configArg = ` --config "${overridePath}"`;
  console.log('🔑 No TAURI_SIGNING_PRIVATE_KEY - skipping updater artifacts');
}

// Build the tauri command
let tauriCmd = `tauri ${command}${configArg}`;
if (feature && feature !== 'none') {
  tauriCmd += ` -- --features ${feature}`;
  console.log(`🚀 Running: tauri ${command} with features: ${feature}`);
} else {
  console.log(`🚀 Running: tauri ${command} (CPU-only mode)`);
}
console.log('');

// Execute the command
try {
  execSync(tauriCmd, { stdio: 'inherit', env });
} catch (err) {
  process.exit(err.status || 1);
}
