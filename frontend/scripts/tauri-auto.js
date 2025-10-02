#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features
 */

const { execSync } = require('child_process');
const path = require('path');

// Get the command (dev or build)
const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build]');
  process.exit(1);
}

// Detect GPU feature
let feature = '';
try {
  const result = execSync('node scripts/auto-detect-gpu.js', {
    encoding: 'utf8',
    stdio: ['pipe', 'pipe', 'inherit']
  });
  feature = result.trim();
} catch (err) {
  // If detection fails, continue with no features
}

console.log(''); // Empty line for spacing

// Build the tauri command
let tauriCmd = `tauri ${command}`;
if (feature) {
  tauriCmd += ` -- --features ${feature}`;
  console.log(`🚀 Running: tauri ${command} with features: ${feature}`);
} else {
  console.log(`🚀 Running: tauri ${command} (CPU-only mode)`);
}
console.log('');

// Execute the command
try {
  execSync(tauriCmd, { stdio: 'inherit' });
} catch (err) {
  process.exit(err.status || 1);
}
