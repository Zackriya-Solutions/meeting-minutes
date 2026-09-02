#!/usr/bin/env node
/**
 * Local Update Testing Server
 *
 * Simple HTTP server to serve latest.json for local OTA update testing.
 * Use this to test the update flow before publishing to GitHub Releases.
 *
 * Usage:
 *   1. Generate latest.json with the manifest generator script
 *   2. Run: node scripts/test-update-locally.js
 *   3. Update tauri.conf.json endpoint to: http://localhost:8080/latest.json
 *   4. Build and run an older version of the app to test updates
 *
 * Press Ctrl+C to stop the server
 */

const http = require('http');
const fs = require('fs');
const path = require('path');

const PORT = 8080;
const LATEST_JSON_PATH = path.join(__dirname, '..', 'latest.json');

console.log('=========================================');
console.log('  PulseTalq Update Testing Server');
console.log('=========================================\n');

// Check if latest.json exists
if (!fs.existsSync(LATEST_JSON_PATH)) {
  console.error(`❌ Error: latest.json not found at ${LATEST_JSON_PATH}`);
  console.error('\nPlease generate it first:');
  console.error('  node scripts/generate-update-manifest-github.js <version>');
  console.error('\nExample:');
  console.error('  node scripts/generate-update-manifest-github.js 0.1.2 \\');
  console.error('    frontend/src-tauri/target/release/bundle/updater \\');
  console.error('    latest.json \\');
  console.error('    "Bug fixes and improvements"');
  process.exit(1);
}

// Read and validate latest.json
let latestJson;
try {
  const content = fs.readFileSync(LATEST_JSON_PATH, 'utf8');
  latestJson = JSON.parse(content);
  console.log('✓ latest.json loaded successfully');
  console.log(`  Version: ${latestJson.version}`);
  console.log(`  Platforms: ${Object.keys(latestJson.platforms).join(', ')}`);
  console.log('');
} catch (error) {
  console.error(`❌ Error reading latest.json: ${error.message}`);
  process.exit(1);
}

// Create HTTP server
const server = http.createServer((req, res) => {
  const timestamp = new Date().toISOString();

  if (req.url === '/latest.json' || req.url === '/') {
    // Serve latest.json with proper CORS headers
    const content = fs.readFileSync(LATEST_JSON_PATH, 'utf8');
    res.writeHead(200, {
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, HEAD',
      'Access-Control-Allow-Headers': 'Content-Type',
      'Cache-Control': 'no-cache, no-store, must-revalidate',
    });
    res.end(content);
    console.log(`[${timestamp}] ✓ Served latest.json (${content.length} bytes)`);
  } else {
    // 404 for other routes
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('Not found');
    console.log(`[${timestamp}] ✗ 404 - ${req.url}`);
  }
});

// Start server
server.listen(PORT, () => {
  console.log('=========================================');
  console.log(`✓ Server running at http://localhost:${PORT}`);
  console.log('=========================================\n');

  console.log('📋 Testing Instructions:\n');

  console.log('1. Update tauri.conf.json endpoint:');
  console.log('   Change the endpoint in frontend/src-tauri/tauri.conf.json to:');
  console.log(`   "endpoints": ["http://localhost:${PORT}/latest.json"]\n`);

  console.log('2. Build an older version:');
  console.log('   - Update version in tauri.conf.json to something older (e.g., 0.1.0)');
  console.log('   - Run: cd frontend && pnpm tauri:build\n');

  console.log('3. Run the app and test updates:');
  console.log('   - The app should detect the update on startup');
  console.log('   - Or use "Check for Updates" from Settings/About');
  console.log('   - Or use "Check for Updates" from system tray\n');

  console.log('4. Verify update flow:');
  console.log('   - Update notification should appear');
  console.log('   - Click to view details');
  console.log('   - Download progress should display');
  console.log('   - App should restart after installation\n');

  console.log('⚠️  IMPORTANT: Restore production endpoint after testing!');
  console.log('   Change back to:');
  console.log('   "endpoints": ["https://github.com/Qblaauw/PulseTalq/releases/latest/download/latest.json"]\n');

  console.log('=========================================');
  console.log('Press Ctrl+C to stop the server');
  console.log('=========================================\n');
});

// Handle server errors
server.on('error', (error) => {
  if (error.code === 'EADDRINUSE') {
    console.error(`❌ Error: Port ${PORT} is already in use`);
    console.error('Please stop the other process or choose a different port');
  } else {
    console.error(`❌ Server error: ${error.message}`);
  }
  process.exit(1);
});

// Handle graceful shutdown
process.on('SIGINT', () => {
  console.log('\n\n=========================================');
  console.log('✓ Server stopped');
  console.log('=========================================\n');
  process.exit(0);
});
