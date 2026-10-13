import { describe, expect, mock, test } from 'bun:test';
import {
  blocksToPlainText,
  createNotesChangeEmitter,
  parseNotesBlocks,
  type NotesContent,
} from '../../src/lib/meeting-notes';

const paragraph = (text: string, children: unknown[] = []) => ({
  id: text,
  type: 'paragraph',
  props: {},
  content: [{ type: 'text', text, styles: {} }],
  children,
});

describe('parseNotesBlocks', () => {
  test('returns the blocks of a stored editor document', () => {
    const blocks = [paragraph('Ship Friday')];
    expect(parseNotesBlocks(JSON.stringify(blocks)) as unknown).toEqual(blocks);
  });

  test('returns null for anything that is not a non-empty block array', () => {
    expect(parseNotesBlocks(null)).toBeNull();
    expect(parseNotesBlocks(undefined)).toBeNull();
    expect(parseNotesBlocks('')).toBeNull();
    expect(parseNotesBlocks('not json')).toBeNull();
    expect(parseNotesBlocks('[]')).toBeNull();
    expect(parseNotesBlocks('{"blocks":[]}')).toBeNull();
    expect(parseNotesBlocks('["text"]')).toBeNull();
    expect(parseNotesBlocks('[{"id":"a"}]')).toBeNull();
  });
});

describe('blocksToPlainText', () => {
  test('collects text from blocks, links and nested blocks, one line per block', () => {
    const blocks = [
      paragraph('Agenda', [paragraph('Budget')]),
      {
        id: 'link',
        type: 'paragraph',
        content: [
          { type: 'text', text: 'See ', styles: {} },
          { type: 'link', href: 'https://example.com', content: [{ type: 'text', text: 'the doc', styles: {} }] },
        ],
        children: [],
      },
      { id: 'empty', type: 'paragraph', content: [], children: [] },
    ];
    expect(blocksToPlainText(blocks)).toBe('Agenda\nBudget\nSee the doc');
  });
});

describe('createNotesChangeEmitter', () => {
  test('emits blocks with their markdown', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => '**Ship** Friday', (c) => emitted.push(c));
    const blocks = [paragraph('Ship Friday')] as any;

    await emitter.handle(blocks);

    expect(emitted).toEqual([{ blocks, markdown: '**Ship** Friday' }]);
  });

  test('falls back to plain text when markdown conversion fails, so the note is not saved as empty', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => undefined, (c) => emitted.push(c));

    await emitter.handle([paragraph('Ship Friday')] as any);

    expect(emitted[0].markdown).toBe('Ship Friday');
  });

  test('drops a slow conversion when a newer edit has arrived', async () => {
    const emitted: NotesContent[] = [];
    let releaseFirst!: (markdown: string) => void;
    const toMarkdown = mock(async (blocks: any[]) =>
      blocks[0].id === 'first'
        ? new Promise<string>((resolve) => { releaseFirst = resolve; })
        : 'second',
    );
    const emitter = createNotesChangeEmitter(toMarkdown as any, (c) => emitted.push(c));

    const first = emitter.handle([paragraph('first')] as any);
    await emitter.handle([paragraph('second')] as any);
    releaseFirst('first');
    await first;

    expect(emitted.map((c) => c.markdown)).toEqual(['second']);
  });

  test('falls back to plain text when markdown conversion returns an empty string', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => '  \n', (c) => emitted.push(c));

    await emitter.handle([paragraph('Ship Friday')] as any);

    expect(emitted[0].markdown).toBe('Ship Friday');
  });

  test('falls back to plain text when markdown conversion rejects', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => { throw new Error('boom'); }, (c) => emitted.push(c));

    await emitter.handle([paragraph('Ship Friday')] as any);

    expect(emitted.map((c) => c.markdown)).toEqual(['Ship Friday']);
  });

  test('a rejected newer conversion does not leave an older edit dropped without a replacement', async () => {
    const emitted: NotesContent[] = [];
    let releaseFirst!: (markdown: string) => void;
    const emitter = createNotesChangeEmitter(async (blocks: any[]) => {
      if (blocks[0].id === 'first') return new Promise<string>((resolve) => { releaseFirst = resolve; });
      throw new Error('boom');
    }, (c) => emitted.push(c));

    const first = emitter.handle([paragraph('first')] as any);
    await emitter.handle([paragraph('second')] as any);
    releaseFirst('first');
    await first;

    expect(emitted.map((c) => c.markdown)).toEqual(['second']);
  });

  test('drops a conversion that was in flight when content was loaded under suppress', async () => {
    const emitted: NotesContent[] = [];
    let release!: (markdown: string) => void;
    const emitter = createNotesChangeEmitter(
      () => new Promise<string>((resolve) => { release = resolve; }),
      (c) => emitted.push(c),
    );

    const typed = emitter.handle([paragraph('typed')] as any);
    await emitter.suppress(async () => {});
    release('typed');
    await typed;

    expect(emitted).toEqual([]);
  });

  test('emits nothing for changes made while suppressed, so opening a note does not rewrite it', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => 'loaded', (c) => emitted.push(c));

    await emitter.suppress(async () => {
      await emitter.handle([paragraph('loaded')] as any);
    });
    expect(emitted).toEqual([]);

    await emitter.handle([paragraph('typed')] as any);
    expect(emitted.length).toBe(1);
  });
});
