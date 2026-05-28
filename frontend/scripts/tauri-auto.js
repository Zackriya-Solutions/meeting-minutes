#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features
 */

const { execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

// Get the command (dev or build)
const args = process.argv.slice(2);
const command = args[0];
const isLocalBuild = args.includes('--local');
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build] [--local]');
  process.exit(1);
}

if (isLocalBuild && command !== 'build') {
  console.error('The --local flag is only supported for build.');
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
let localConfigArg = '';

if (isLocalBuild) {
  let tauriConfig = {};
  if (process.env.TAURI_CONFIG) {
    try {
      tauriConfig = JSON.parse(process.env.TAURI_CONFIG);
    } catch (err) {
      console.warn('⚠️  Existing TAURI_CONFIG is not valid JSON; replacing it for local build config.');
    }
  }

  const localConfig = {
    ...tauriConfig,
    bundle: {
      ...(tauriConfig.bundle || {}),
      createUpdaterArtifacts: false,
    },
  };
  const escapedConfig = JSON.stringify(localConfig).replace(/'/g, `'\\''`);
  localConfigArg = ` --config '${escapedConfig}'`;
  console.log('🔓 Local build: updater signing artifacts disabled');
}

if (platform === 'linux' && feature === 'cuda') {
  console.log('🐧 Linux/CUDA detected: Setting CMAKE flags for NVIDIA GPU');
  env.CMAKE_CUDA_ARCHITECTURES = '75';
  env.CMAKE_CUDA_STANDARD = '17';
  env.CMAKE_POSITION_INDEPENDENT_CODE = 'ON';
}

// Build the tauri command
let tauriCmd = `tauri ${command}${localConfigArg}`;
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
