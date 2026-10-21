const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');
const { test } = require('node:test');

const guard = join(__dirname, 'verify-portable-ggml.cjs');
const portableCache = `GGML_NATIVE:BOOL=OFF
GGML_AVX:BOOL=ON
GGML_AVX2:BOOL=ON
GGML_AVX512:BOOL=OFF
GGML_AVX512_VBMI:BOOL=OFF
GGML_AVX512_VNNI:BOOL=OFF
GGML_AVX512_BF16:BOOL=OFF
`;

function runGuard(t, caches, buildPath = 'target/release') {
  const root = mkdtempSync(join(tmpdir(), 'portable-ggml-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const [path, content] of Object.entries(caches)) {
    const directory = join(root, path, 'out/build');
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, 'CMakeCache.txt'), content);
  }
  return spawnSync(process.execPath, [guard, buildPath], { cwd: root, encoding: 'utf8' });
}

test('accepts portable Whisper configuration with AVX2 enabled', (t) => {
  const result = runGuard(t, { 'target/release/build/whisper-rs-sys-abc': portableCache });
  assert.equal(result.status, 0, result.stderr);
});

test('checks an explicitly selected target and debug profile', (t) => {
  const result = runGuard(t, {
    'target/x86_64-pc-windows-msvc/debug/build/whisper-rs-sys-abc': portableCache,
    'target/release/build/whisper-rs-sys-old': portableCache.replace('GGML_NATIVE:BOOL=OFF', 'GGML_NATIVE:BOOL=ON'),
  }, 'target/x86_64-pc-windows-msvc/debug');
  assert.equal(result.status, 0, result.stderr);
});

test('rejects missing Whisper evidence even if another dependency is portable', (t) => {
  const result = runGuard(t, { 'target/release/build/llama-cpp-sys-abc': portableCache });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /No Whisper CMake cache/);
});

for (const flag of ['GGML_NATIVE', 'GGML_AVX512', 'GGML_AVX512_VBMI', 'GGML_AVX512_VNNI', 'GGML_AVX512_BF16']) {
  test(`rejects ${flag} enabled`, (t) => {
    const result = runGuard(t, { 'target/release/build/whisper-rs-sys-abc': portableCache.replace(`${flag}:BOOL=OFF`, `${flag}:BOOL=ON`) });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(`${flag} must be explicitly OFF`));
  });

  test(`rejects missing ${flag} evidence`, (t) => {
    const result = runGuard(t, { 'target/release/build/whisper-rs-sys-abc': portableCache.replace(`${flag}:BOOL=OFF\n`, '') });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(`${flag} must be explicitly OFF`));
  });
}

test('does not let one safe cache hide an unsafe cache in the selected build', (t) => {
  const result = runGuard(t, {
    'target/release/build/whisper-rs-sys-abc': portableCache,
    'target/release/build/whisper-rs-sys-def': portableCache.replace('GGML_NATIVE:BOOL=OFF', 'GGML_NATIVE:BOOL=ON'),
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /GGML_NATIVE must be explicitly OFF/);
});
