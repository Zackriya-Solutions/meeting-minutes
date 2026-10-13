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

/** Plain text of a block tree, one line per block that has text. */
export function blocksToPlainText(blocks: unknown[]): string {
  const lines: string[] = [];
  const visit = (block: unknown) => {
    if (!block || typeof block !== 'object') return;
    const { content, children } = block as { content?: unknown; children?: unknown };
    if (Array.isArray(content)) {
      const text = content.map(inlineText).join('');
      if (text.trim()) lines.push(text);
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
}

export function createNotesChangeEmitter(
  toMarkdown: (blocks: Block[]) => Promise<string | undefined>,
  emit: (content: NotesContent) => void,
): NotesChangeEmitter {
  let sequence = 0;
  let suppressed = 0;

  return {
    async handle(blocks) {
      if (suppressed > 0) return;
      const current = ++sequence;
      let markdown: string | undefined;
      try {
        markdown = await toMarkdown(blocks);
      } catch {
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
