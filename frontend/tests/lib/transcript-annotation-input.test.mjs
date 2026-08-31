import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'transcript-annotation-input.ts'
);
const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, {
  exports: cjsModule.exports,
  module: cjsModule,
  btoa: value => Buffer.from(value, 'binary').toString('base64'),
});
const {
  formatClipboardImageError,
  imageDataToDataUrl,
  preserveLiveAnnotationImageData,
  readClipboardImageAnnotation,
  resolveTranscriptAnchor,
} = cjsModule.exports;
const componentSource = fs.readFileSync(
  path.join(path.dirname(modulePath), '..', 'components', 'VirtualizedTranscriptView.tsx'),
  'utf8'
);

assert.match(componentSource, /readClipboardImageAnnotation\(\s*readImage,\s*anchorTime/, 'the transcript view must read system clipboard images');
assert.match(componentSource, /document\.addEventListener\('keydown', handleGlobalPaste\)/, 'Ctrl/Cmd+V must be handled independently of transcript focus');
assert.match(componentSource, /Colar imagem/, 'the transcript controls must expose a native image paste button');
assert.match(componentSource, /role="alert"/, 'clipboard failures must be visible to the user');
assert.match(componentSource, /invoke<number\[\] \| null>\('read_wayland_clipboard_image'\)/, 'native clipboard failures must use the Wayland fallback');
assert.match(componentSource, /aria-current=\{isActive \? 'true' : undefined\}/, 'the selected transcript anchor must be visually identifiable');
assert.match(componentSource, /imageDataToDataUrl\(annotation\.imageData/, 'live previews must use the CSP-safe data URL path');

const liveImageSource = imageDataToDataUrl([137, 80, 78, 71], 'image/png');
assert.match(liveImageSource, /^data:image\/png;base64,/, 'live image previews must use a data URL');
assert.equal(liveImageSource, 'data:image/png;base64,iVBORw==', 'live image previews must encode their bytes as base64');
assert.doesNotMatch(liveImageSource, /^blob:/, 'live image previews must not use CSP-blocked blob URLs');

const persistedImage = {
  id: 'annotation-1',
  type: 'image',
  anchorTime: 4,
  createdAt: '2026-08-31T20:00:00.000Z',
  imageFile: 'annotation-1.png',
};
const liveImage = preserveLiveAnnotationImageData(persistedImage, {
  type: 'image',
  anchorTime: 4,
  imageData: [137, 80, 78, 71],
  imageMime: 'image/png',
});
assert.deepEqual(JSON.parse(JSON.stringify(liveImage.imageData)), [137, 80, 78, 71], 'live image bytes must survive a persistence response that only contains the image file');
assert.equal(liveImage.imageMime, 'image/png', 'live image MIME type must survive persistence normalization');

const encodedPng = await readClipboardImageAnnotation(
  async () => ({
    rgba: async () => new Uint8Array([255, 0, 0, 255]),
    size: async () => ({ width: 1, height: 1 }),
  }),
  12.5,
  async (rgba, size) => {
    assert.deepEqual(Array.from(rgba), [255, 0, 0, 255]);
    assert.deepEqual(size, { width: 1, height: 1 });
    return new Uint8Array([137, 80, 78, 71]);
  }
);
assert.deepEqual(JSON.parse(JSON.stringify(encodedPng)), {
  type: 'image',
  anchorTime: 12.5,
  imageData: [137, 80, 78, 71],
  imageMime: 'image/png',
}, 'clipboard image reads must become a PNG annotation at the selected anchor');

const fallbackPng = await readClipboardImageAnnotation(
  async () => { throw new Error('native clipboard unavailable'); },
  7.5,
  async () => { throw new Error('native encoder should not run for PNG fallback'); },
  async () => new Uint8Array([137, 80, 78, 71])
);
assert.deepEqual(JSON.parse(JSON.stringify(fallbackPng)), {
  type: 'image',
  anchorTime: 7.5,
  imageData: [137, 80, 78, 71],
  imageMime: 'image/png',
}, 'Wayland PNG fallback must use the same annotation payload');

assert.equal(
  await readClipboardImageAnnotation(
    async () => { throw new Error('native clipboard unavailable'); },
    7.5,
    async () => { throw new Error('native encoder should not run for an empty fallback'); },
    async () => null
  ),
  null,
  'an empty Wayland clipboard must be treated as no image'
);

await assert.rejects(
  () => readClipboardImageAnnotation(async () => { throw new Error('clipboard has no image'); }, 12.5),
  /clipboard has no image/,
  'clipboard failures must preserve the native error for the UI'
);
assert.match(
  formatClipboardImageError(new Error('clipboard unavailable')),
  /^Falha ao colar imagem: clipboard unavailable\. .*Linux\/Wayland/,
  'clipboard failures must include a useful Linux/Wayland hint'
);

assert.deepEqual(
  JSON.parse(JSON.stringify(resolveTranscriptAnchor(42, { timestamp: 10, endTime: 12 }))),
  { time: 42, isDefault: false, label: '[00:42] selected transcript point' },
  'an explicit transcript selection is the annotation anchor'
);
assert.deepEqual(
  JSON.parse(JSON.stringify(resolveTranscriptAnchor(null, { timestamp: 10, endTime: 12 }))),
  { time: 12, isDefault: true, label: '[00:12] latest transcript point (default)' },
  'without a selection annotations use the latest transcript point and say so'
);

console.log('transcript-annotation-input tests passed');
