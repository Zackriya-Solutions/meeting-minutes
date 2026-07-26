// Unit tests for the canonical export markdown builder.
//
// Uses node:test rather than the bun:test convention of the sibling files so it
// runs with the toolchain already required to build the app. The module under
// test has no runtime imports, so Node's type stripping is enough to load it.

import assert from 'node:assert/strict';
import { describe, test } from 'node:test';

import {
  buildExportFileName,
  buildExportMarkdown,
  formatTranscriptTime,
  summaryToMarkdown,
  transcriptsToMarkdown,
} from '../../src/lib/export-markdown.ts';

const meeting = {
  id: 'meeting-1',
  title: 'Sprint Planlama',
  created_at: '2026-07-26T09:15:00.000Z',
};

const transcripts = [
  { id: '1', text: 'Merhaba, başlıyoruz.', timestamp: '14:30:05', audio_start_time: 0 },
  { id: '2', text: 'Ölçüm panosu hazır.', timestamp: '14:31:12', audio_start_time: 67 },
];

describe('formatTranscriptTime', () => {
  test('renders recording-relative MM:SS', () => {
    assert.equal(formatTranscriptTime(0, '14:30:05'), '[00:00]');
    assert.equal(formatTranscriptTime(67, '14:31:12'), '[01:07]');
    assert.equal(formatTranscriptTime(3599, 'x'), '[59:59]');
  });

  test('rolls past an hour into minutes rather than truncating', () => {
    assert.equal(formatTranscriptTime(3600, 'x'), '[60:00]');
  });

  test('falls back to the wall-clock stamp for legacy rows', () => {
    assert.equal(formatTranscriptTime(undefined, '14:30:05'), '14:30:05');
    assert.equal(formatTranscriptTime(Number.NaN, '14:30:05'), '14:30:05');
  });
});

describe('summaryToMarkdown', () => {
  test('prefers the markdown field written by the current pipeline', () => {
    const summary = { markdown: '## Özet\n\nHer şey yolunda.' };
    assert.equal(summaryToMarkdown(summary), '## Özet\n\nHer şey yolunda.');
  });

  test('converts the legacy section shape', () => {
    const summary = {
      key_points: { title: 'Key Points', blocks: [{ content: 'First' }, { content: 'Second' }] },
      action_items: { title: 'Action Items', blocks: [{ content: 'Ship it' }] },
    };
    assert.equal(
      summaryToMarkdown(summary),
      '## Key Points\n\n- First\n- Second\n\n## Action Items\n\n- Ship it'
    );
  });

  test('skips bookkeeping keys and empty sections', () => {
    const summary = {
      MeetingName: { title: 'x', blocks: [{ content: 'ignored' }] },
      _section_order: { title: 'x', blocks: [{ content: 'ignored' }] },
      empty: { title: 'Empty', blocks: [] },
      real: { title: 'Real', blocks: [{ content: 'kept' }] },
    };
    assert.equal(summaryToMarkdown(summary), '## Real\n\n- kept');
  });

  test('returns an empty string when there is nothing', () => {
    assert.equal(summaryToMarkdown(null), '');
    assert.equal(summaryToMarkdown({}), '');
    assert.equal(summaryToMarkdown({ markdown: '   ' }), '');
  });
});

describe('transcriptsToMarkdown', () => {
  test('renders one timestamped block per segment', () => {
    assert.equal(
      transcriptsToMarkdown(transcripts),
      '[00:00] Merhaba, başlıyoruz.\n\n[01:07] Ölçüm panosu hazır.'
    );
  });

  test('drops blank segments', () => {
    const withBlank = [...transcripts, { id: '3', text: '   ', timestamp: 'x', audio_start_time: 90 }];
    assert.equal(transcriptsToMarkdown(withBlank).split('\n\n').length, 2);
  });
});

describe('buildExportMarkdown', () => {
  const base = { meeting, title: 'Sprint Planlama', summary: { markdown: '## Özet\n\nKısa.' }, transcripts };

  test('summary scope has no transcript and no Summary heading', () => {
    const md = buildExportMarkdown({ ...base, scope: 'summary' });
    assert.match(md, /^# Sprint Planlama\n/);
    assert.ok(md.includes('## Özet'));
    assert.ok(!md.includes('## Transcript'));
    assert.ok(!md.includes('## Summary'));
  });

  test('transcript scope has no summary', () => {
    const md = buildExportMarkdown({ ...base, scope: 'transcript' });
    assert.ok(md.includes('## Transcript'));
    assert.ok(md.includes('[01:07] Ölçüm panosu hazır.'));
    assert.ok(!md.includes('## Özet'));
  });

  test('both scope labels each part and keeps summary first', () => {
    const md = buildExportMarkdown({ ...base, scope: 'both' });
    assert.ok(md.indexOf('## Summary') < md.indexOf('## Transcript'));
    assert.ok(md.includes('## Özet'));
  });

  test('unsaved editor markdown wins over the stored summary', () => {
    const md = buildExportMarkdown({ ...base, scope: 'summary', summaryMarkdown: '## Taze\n\nYeni.' });
    assert.ok(md.includes('## Taze'));
    assert.ok(!md.includes('## Özet'));
  });

  test('always carries a date header', () => {
    const md = buildExportMarkdown({ ...base, scope: 'summary' });
    assert.ok(md.includes('**Date:**'));
    assert.ok(md.includes('**Exported:**'));
  });

  test('refuses to build an empty document', () => {
    assert.throws(
      () => buildExportMarkdown({ ...base, scope: 'summary', summary: null, summaryMarkdown: '' }),
      /No summary content/
    );
    assert.throws(
      () => buildExportMarkdown({ ...base, scope: 'transcript', transcripts: [] }),
      /No transcript content/
    );
    assert.throws(
      () => buildExportMarkdown({ ...base, scope: 'both', transcripts: [] }),
      /No transcript content/
    );
  });

  test('falls back to the meeting title when the edited title is blank', () => {
    const md = buildExportMarkdown({ ...base, title: '', scope: 'summary' });
    assert.match(md, /^# Sprint Planlama\n/);
  });
});

describe('buildExportFileName', () => {
  test('combines title, date and scope', () => {
    assert.equal(
      buildExportFileName('Sprint Planlama', meeting.created_at, 'summary'),
      'Sprint Planlama-2026-07-26-summary'
    );
    assert.equal(
      buildExportFileName('Sprint Planlama', meeting.created_at, 'transcript'),
      'Sprint Planlama-2026-07-26-transcript'
    );
    assert.equal(
      buildExportFileName('Sprint Planlama', meeting.created_at, 'both'),
      'Sprint Planlama-2026-07-26-full'
    );
  });

  test('survives a missing title and an unparseable date', () => {
    assert.equal(buildExportFileName('', 'not-a-date', 'summary'), 'meeting-summary');
  });
});
