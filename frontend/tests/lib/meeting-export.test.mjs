import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'meeting-export.ts'
);
const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, {
  exports: cjsModule.exports,
  module: cjsModule,
  require: moduleName => {
    if (moduleName === '@/lib/transcript-annotation-input') {
      return {
        imageDataToDataUrl: (imageData, mime = 'image/png') =>
          `data:${mime};base64,${Buffer.from(Uint8Array.from(imageData)).toString('base64')}`,
      };
    }
    throw new Error(`Unexpected module: ${moduleName}`);
  },
  btoa: value => Buffer.from(value, 'binary').toString('base64'),
});

const { buildMeetingMarkdown } = cjsModule.exports;
const markdown = buildMeetingMarkdown({
  title: 'Planning & Review',
  date: 'August 31, 2026',
  summaryMarkdown: 'The team agreed to ship the export flow.',
  segments: [
    { id: 'second', text: 'The follow-up starts now.', audio_start_time: 12, source: 'Outros' },
    { id: 'first', text: 'We reviewed the plan.', audio_start_time: 4, source: 'Você' },
  ],
  annotations: [
    { id: 'note-1', type: 'note', anchorTime: 6, text: 'Confirm the PDF layout.', createdAt: '2026-08-31T20:00:00.000Z' },
    { id: 'image-1', type: 'image', anchorTime: 14, text: 'Whiteboard', imageData: [137, 80, 78, 71], imageMime: 'image/png', createdAt: '2026-08-31T20:01:00.000Z' },
  ],
});

assert.match(markdown, /^# Planning & Review\n\n## Date: August 31, 2026/m);
assert.match(markdown, /## AI Summary\n\nThe team agreed to ship the export flow\./);
assert.ok(markdown.indexOf('[00:04] Você: We reviewed the plan.') < markdown.indexOf('[00:06] Note: Confirm the PDF layout.'));
assert.ok(markdown.indexOf('[00:06] Note: Confirm the PDF layout.') < markdown.indexOf('[00:12] Outros: The follow-up starts now.'));
assert.match(markdown, /!\[Whiteboard \[00:14\]\]\(data:image\/png;base64,iVBORw==\)/);

console.log('meeting export tests passed');
