const { existsSync, readdirSync, readFileSync } = require('node:fs');
const { join, resolve } = require('node:path');

// Run before bundling so unverified native code cannot become a release asset.
try {
  if (process.argv.length !== 3) {
    throw new Error('Usage: node verify-portable-ggml.cjs <target/profile directory>');
  }
  const buildDirectory = resolve(process.argv[2], 'build');
  const caches = existsSync(buildDirectory)
    ? readdirSync(buildDirectory, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && entry.name.startsWith('whisper-rs-sys-'))
      .map((entry) => join(buildDirectory, entry.name, 'out/build/CMakeCache.txt'))
      .filter(existsSync)
    : [];
  if (caches.length === 0) {
    throw new Error(`No Whisper CMake cache found in ${buildDirectory}`);
  }

  for (const cache of caches) {
    const contents = readFileSync(cache, 'utf8');
    console.log(`Checking ${cache}`);
    for (const line of contents.split(/\r?\n/)) {
      if (/^(GGML_(NATIVE|AVX[^:]*|FMA|F16C)|CMAKE_C(?:XX)?_FLAGS[^:]*):/.test(line)) {
        console.log(line);
      }
    }
    for (const flag of ['GGML_NATIVE', 'GGML_AVX512', 'GGML_AVX512_VBMI', 'GGML_AVX512_VNNI', 'GGML_AVX512_BF16']) {
      if (!new RegExp(`^${flag}:BOOL=OFF\\r?$`, 'm').test(contents)) {
        throw new Error(`${flag} must be explicitly OFF in ${cache}`);
      }
    }
  }
  console.log('Verified Windows Whisper CPU portability before bundling.');
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
