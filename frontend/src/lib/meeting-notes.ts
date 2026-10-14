import type { Block } from '@blocknote/core';

/** A note as the editor holds it, plus the same content as markdown. */
export interface NotesContent {
  blocks: Block[];
  markdown: string;
}

/**
 * Parses a stored editor document.
 * Returns null when it is missing or is not a non-empty array of blocks.
 */
export function parseNotesBlocks(notesJson: string | null | undefined): Block[] | null {
  if (!notesJson) return null;
  try {
    const parsed: unknown = JSON.parse(notesJson);
    if (!Array.isArray(parsed) || parsed.length === 0) return null;
    const allBlocks = parsed.every(
      (block) =>
        block !== null &&
        typeof block === 'object' &&
        typeof (block as { type?: unknown }).type === 'string',
    );
    return allBlocks ? (parsed as Block[]) : null;
  } catch {
    return null;
  }
}

function inlineText(node: unknown): string {
  if (!node || typeof node !== 'object') return '';
  const { text, content } = node as { text?: unknown; content?: unknown };
  if (typeof text === 'string') return text;
  return Array.isArray(content) ? content.map(inlineText).join('') : '';
}

/** True for a block that can hold text, holds none, and has no child blocks. */
export function isEmptyTextBlock(block: unknown): boolean {
  if (!block || typeof block !== 'object') return false;
  const { content, children } = block as { content?: unknown; children?: unknown };
  if (!Array.isArray(content)) return false;
  if (Array.isArray(children) && children.length > 0) return false;
  return content.map(inlineText).join('').trim() === '';
}

/** Plain text of a block tree, one line per block that has text. */
export function blocksToPlainText(blocks: unknown[]): string {
  const lines: string[] = [];
  const visit = (block: unknown) => {
    if (!block || typeof block !== 'object') return;
    const { content, children } = block as { content?: unknown; children?: unknown };
    if (Array.isArray(content)) {
      const text = content.map(inlineText).join('');
      if (text.trim()) lines.push(text);
    } else if (content && typeof content === 'object') {
      const { rows } = content as { rows?: unknown };
      if (Array.isArray(rows)) {
        for (const row of rows) {
          const cells = (row as { cells?: unknown } | null)?.cells;
          if (!Array.isArray(cells)) continue;
          const texts = cells.map((cell) => {
            const cellContent = Array.isArray(cell) ? cell : (cell as { content?: unknown } | null)?.content;
            return Array.isArray(cellContent) ? cellContent.map(inlineText).join('') : '';
          });
          if (texts.some((text) => text.trim())) lines.push(texts.join(' | '));
        }
      }
    }
    if (Array.isArray(children)) children.forEach(visit);
  };
  blocks.forEach(visit);
  return lines.join('\n');
}

export interface NotesChangeEmitter {
  /** Converts an edit to markdown and emits it, unless a newer edit arrived meanwhile. */
  handle: (blocks: Block[]) => Promise<void>;
  /** Runs `work` while ignoring edits, for content the app loads into the editor itself. */
  suppress: <T>(work: () => Promise<T>) => Promise<T>;
  /** Records the document the editor opened with; edits identical to it are ignored until one differs. */
  markLoaded: (blocks: Block[]) => void;
}

/**
 * Decides what to do when the editor fails: only a failure before the editor was ever
 * ready, while opening stored blocks, is worth retrying from markdown.
 */
export function editorFailureAction(
  editorWasReady: boolean,
  fromMarkdown: boolean,
): 'open-from-markdown' | 'show-error' {
  return !editorWasReady && !fromMarkdown ? 'open-from-markdown' : 'show-error';
}

export function createNotesChangeEmitter(
  toMarkdown: (blocks: Block[]) => Promise<string | undefined>,
  emit: (content: NotesContent) => void,
): NotesChangeEmitter {
  let sequence = 0;
  let suppressed = 0;
  let loadedSnapshot: string | null = null;

  return {
    markLoaded(blocks) {
      loadedSnapshot = JSON.stringify(blocks);
    },
    async handle(blocks) {
      if (suppressed > 0) return;
      if (loadedSnapshot !== null) {
        if (JSON.stringify(blocks) === loadedSnapshot) return;
        loadedSnapshot = null;
      }
      const current = ++sequence;
      let markdown: string | undefined;
      try {
        markdown = await toMarkdown(blocks);
      } catch (error) {
        console.error('Failed to convert notes to markdown', error);
        markdown = undefined;
      }
      if (current !== sequence) return;
      emit({ blocks, markdown: markdown?.trim() ? markdown : blocksToPlainText(blocks) });
    },
    async suppress(work) {
      // Conversions already running describe content the editor no longer holds.
      sequence += 1;
      suppressed += 1;
      try {
        return await work();
      } finally {
        suppressed -= 1;
      }
    },
  };
}
