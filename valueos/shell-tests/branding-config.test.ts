import { describe, it, expect } from 'vitest';
import { readFileSync, mkdtempSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// VALUEOS: guards the merge-safe CSP fix. In a packaged Tauri build, Tauri injects a
// nonce into the style-src CSP directive, which (per the CSP spec) nullifies
// 'unsafe-inline' and silently strips every React inline style= attribute — rendering our
// screens completely unstyled. The branding overlay exempts style-src from that
// modification. These tests fail if that exemption is ever dropped or the generator stops
// carrying it into the CI config.

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '../..');
const brandingDir = path.resolve(repoRoot, 'valueos/branding');
const overlaySrc = path.resolve(brandingDir, 'tauri.valueos.json');
const generator = path.resolve(brandingDir, 'make-ci-config.js');
const upstreamConf = path.resolve(repoRoot, 'frontend/src-tauri/tauri.conf.json');
const branchWorkflow = path.resolve(repoRoot, '.github/workflows/valueos-branch.yml');
const mainWorkflow = path.resolve(repoRoot, '.github/workflows/valueos-main.yml');

function readJson(p: string) {
  return JSON.parse(readFileSync(p, 'utf8'));
}

describe('branding overlay CSP fix', () => {
  it('the overlay exempts style-src from Tauri CSP modification', () => {
    const overlay = readJson(overlaySrc);
    const exempt = overlay?.app?.security?.dangerousDisableAssetCspModification;
    expect(Array.isArray(exempt)).toBe(true);
    expect(exempt).toContain('style-src');
    // script-src must NOT be exempted — script nonce-hardening stays intact.
    expect(exempt).not.toContain('script-src');
  });

  it('upstream CSP still allows inline styles, so the exemption is meaningful', () => {
    const conf = readJson(upstreamConf);
    const styleSrc: string = conf?.app?.security?.csp?.['style-src'] ?? '';
    expect(styleSrc).toContain("'unsafe-inline'");
  });

  it('the CI config generator carries the exemption end-to-end (+ unsigned build override)', () => {
    const outDir = mkdtempSync(path.join(tmpdir(), 'valueos-ci-'));
    const outPath = path.join(outDir, 'valueos-ci.config.json');
    execFileSync('node', [generator, outPath], { cwd: brandingDir });
    const ci = readJson(outPath);
    expect(ci.app.security.dangerousDisableAssetCspModification).toContain('style-src');
    expect(ci.bundle.createUpdaterArtifacts).toBe(false);
    // The top-level $comment is stripped so Tauri doesn't reject an unknown field.
    expect(ci.$comment).toBeUndefined();
  });

  it('the overlay pins the ValueOS Agent version to 0.0.1', () => {
    expect(readJson(overlaySrc).version).toBe('0.0.1');
  });

  it('every build workflow forces REAL transport (no accidental mock in packaged builds)', () => {
    // Inlined at build so the packaged app never falls back to the Acme/Ada mock seed.
    for (const wf of [branchWorkflow, mainWorkflow]) {
      expect(readFileSync(wf, 'utf8')).toMatch(/NEXT_PUBLIC_VALUEOS_REAL\s*=\s*"?on"?/);
    }
  });
});
