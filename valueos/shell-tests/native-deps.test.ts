import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// VALUEOS: guards login token persistence. keyring v3 silently uses a NON-persistent mock
// store unless a platform backend feature is enabled — which makes login "succeed" but the
// next command report "Not logged in". These tests fail if the real OS-keychain backends
// are ever dropped from the agent's native deps.

const here = path.dirname(fileURLToPath(import.meta.url));
const cargo = readFileSync(
  path.resolve(here, '../..', 'frontend/src-tauri/Cargo.toml'),
  'utf8',
);

describe('keyring native backends (OAuth token persistence)', () => {
  it('enables the real macOS Keychain backend (apple-native)', () => {
    expect(cargo).toMatch(
      /keyring\s*=\s*\{[^}]*features\s*=\s*\[[^\]]*"apple-native"[^\]]*\]/,
    );
  });

  it('enables the real Windows Credential Manager backend (windows-native)', () => {
    expect(cargo).toMatch(
      /keyring\s*=\s*\{[^}]*features\s*=\s*\[[^\]]*"windows-native"[^\]]*\]/,
    );
  });
});
