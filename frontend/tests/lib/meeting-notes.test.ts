import { describe, expect, mock, test } from 'bun:test';
import {
  blocksToPlainText,
  createNotesChangeEmitter,
  editorFailureAction,
  isEmptyTextBlock,
  parseNotesBlocks,
  type NotesContent,
} from '../../src/lib/meeting-notes';
import { withQuietConsoleError } from '../helpers/quiet-console';

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

  const inline = (text: string) => [{ type: 'text', text, styles: {} }];
  const table = (rows: unknown[][]) => ({
    id: 'table',
    type: 'table',
    content: { type: 'tableContent', rows: rows.map((cells) => ({ cells })) },
    children: [],
  });

  test('collects table rows, joining cells with a bar, for object-shaped cells', () => {
    const cell = (text: string) => ({ type: 'tableCell', content: inline(text), props: {} });
    const blocks = [table([[cell('A'), cell('B')], [cell('C'), cell('D')]])];
    expect(blocksToPlainText(blocks)).toBe('A | B\nC | D');
  });

  test('collects table rows for array-shaped cells', () => {
    const blocks = [table([[inline('A'), inline('B')], [inline('C'), inline('D')]])];
    expect(blocksToPlainText(blocks)).toBe('A | B\nC | D');
  });

  test('a table row whose cells are all empty adds no line', () => {
    const blocks = [table([[inline(''), inline('')], [inline('C'), inline('D')]])];
    expect(blocksToPlainText(blocks)).toBe('C | D');
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

    await withQuietConsoleError(() => emitter.handle([paragraph('Ship Friday')] as any));

    expect(emitted.map((c) => c.markdown)).toEqual(['Ship Friday']);
  });

  test('a rejected newer conversion does not leave an older edit dropped without a replacement', async () => {
    const emitted: NotesContent[] = [];
    let releaseFirst!: (markdown: string) => void;
    const emitter = createNotesChangeEmitter(async (blocks: any[]) => {
      if (blocks[0].id === 'first') return new Promise<string>((resolve) => { releaseFirst = resolve; });
      throw new Error('boom');
    }, (c) => emitted.push(c));

    await withQuietConsoleError(async () => {
      const first = emitter.handle([paragraph('first')] as any);
      await emitter.handle([paragraph('second')] as any);
      releaseFirst('first');
      await first;
    });

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

  test('markLoaded ignores changes identical to the loaded document until one differs', async () => {
    const emitted: NotesContent[] = [];
    const emitter = createNotesChangeEmitter(async () => 'md', (c) => emitted.push(c));
    emitter.markLoaded([paragraph('loaded')] as any);

    await emitter.handle([paragraph('loaded')] as any);
    expect(emitted).toEqual([]);

    await emitter.handle([paragraph('edited')] as any);
    expect(emitted.length).toBe(1);

    await emitter.handle([paragraph('loaded')] as any);
    expect(emitted.length).toBe(2);
  });
});

describe('editorFailureAction', () => {
  test('opens from markdown only when the editor failed before it was ever ready and was not already in markdown mode', () => {
    expect(editorFailureAction(false, false)).toBe('open-from-markdown');
    expect(editorFailureAction(false, true)).toBe('show-error');
    expect(editorFailureAction(true, false)).toBe('show-error');
    expect(editorFailureAction(true, true)).toBe('show-error');
  });
});

describe('isEmptyTextBlock', () => {
  test('is true for an empty paragraph', () => {
    expect(isEmptyTextBlock({ type: 'paragraph', content: [], children: [] })).toBe(true);
  });

  test('is true for a paragraph holding only spaces', () => {
    expect(isEmptyTextBlock({ type: 'paragraph', content: [{ type: 'text', text: '  ' }], children: [] })).toBe(true);
  });

  test('is false for a paragraph with text', () => {
    expect(isEmptyTextBlock(paragraph('hello'))).toBe(false);
  });

  test('is false for an empty paragraph with a child block', () => {
    expect(isEmptyTextBlock({ type: 'paragraph', content: [], children: [paragraph('child')] })).toBe(false);
  });

  test('is false for a block whose content is not an array', () => {
    expect(isEmptyTextBlock({ type: 'table', content: { type: 'tableContent', rows: [] }, children: [] })).toBe(false);
  });

  test('is false for values that are not blocks', () => {
    expect(isEmptyTextBlock(null)).toBe(false);
    expect(isEmptyTextBlock('text')).toBe(false);
  });
});
