#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features.
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
    // If detection fails, continue with no features.
  }
}

console.log('');

// Platform-specific environment variables
const platform = os.platform();
const env = { ...process.env };

if (platform === 'linux' && feature === 'cuda') {
  console.log('🐧 Linux/CUDA detected: Setting CMAKE flags for NVIDIA GPU');
  env.CMAKE_CUDA_ARCHITECTURES = '75';
  env.CMAKE_CUDA_STANDARD = '17';
  env.CMAKE_POSITION_INDEPENDENT_CODE = 'ON';
}

// Build the tauri command
let tauriCmd = `tauri ${command}`;

if (command === 'build') {
  const generatedConfig = { bundle: {} };

  if (!env.TAURI_SIGNING_PRIVATE_KEY) {
    generatedConfig.bundle.createUpdaterArtifacts = false;
    console.log('🔓 No TAURI_SIGNING_PRIVATE_KEY detected - disabling updater artifacts for local build');
  }

  if (env.DIGICERT_KEYPAIR_ALIAS) {
    generatedConfig.bundle.windows = {
      signCommand: 'powershell -ExecutionPolicy Bypass -File scripts/sign-windows.ps1 -FilePath %1'
    };
    console.log('🔏 DIGICERT_KEYPAIR_ALIAS detected - enabling Windows code signing');
  } else if (platform === 'win32') {
    console.log('🔓 No DIGICERT_KEYPAIR_ALIAS detected - skipping Windows code signing for local build');
  }

  const generatedConfigPath = path.join('src-tauri', 'tauri.generated.conf.json');
  fs.writeFileSync(generatedConfigPath, `${JSON.stringify(generatedConfig, null, 2)}\n`);
  tauriCmd += ` --config ${generatedConfigPath}`;
}

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
