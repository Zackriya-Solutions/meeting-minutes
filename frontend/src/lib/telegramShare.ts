/**
 * Turning a meeting summary into the plain text a Telegram share link can carry.
 *
 * The share deep link (`tg://msg?text=…`) delivers a *plain* message — Telegram applies
 * no Markdown or HTML parsing to it — so the Markdown is flattened here rather than
 * translated to another markup. Formatting that cannot survive (bold, headings) is
 * dropped; structure that can (bullets, checkboxes, blank lines between sections) is
 * kept with typographic characters.
 */

/**
 * Longest single message Telegram accepts. Past this the pasted body will not send as one
 * message, so a `.md` file is produced alongside it.
 */
export const TELEGRAM_MESSAGE_LIMIT = 4096;

/**
 * Characters the `tg://` draft may carry — a title and date, nothing more. Kept far below
 * the point where Telegram starts mangling long links; see the Rust `telegram::share`
 * module for the measured thresholds. Must not exceed `DRAFT_TEXT_BUDGET` there.
 */
export const TELEGRAM_DRAFT_BUDGET = 400;

/** Strip inline Markdown emphasis, code, and link syntax, keeping the readable text. */
function stripInline(input: string): string {
  return (
    input
      // Images first — their alt text is all that survives.
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      // Links keep the target: a bare label in a chat message is a dead end.
      .replace(/\[([^\]]+)\]\(\s*([^)\s]+)[^)]*\)/g, '$1 ($2)')
      .replace(/`([^`]*)`/g, '$1')
      // Bold before italic, or the italic pass eats one marker of a bold pair.
      .replace(/\*\*([^*]+)\*\*/g, '$1')
      .replace(/__([^_]+)__/g, '$1')
      .replace(/\*([^*\n]+)\*/g, '$1')
      // Underscore italics only at word boundaries, so snake_case_names survive.
      .replace(/(^|[\s(])_([^_\n]+)_(?=[\s).,;:!?]|$)/g, '$1$2')
      .replace(/~~([^~]+)~~/g, '$1')
  );
}

/** Flatten summary Markdown into Telegram-ready plain text. */
export function markdownToTelegramText(markdown: string): string {
  const lines = (markdown || '').replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let inFence = false;

  for (const raw of lines) {
    const line = raw.replace(/\s+$/, '');

    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence;
      continue;
    }
    // Code blocks go through verbatim — indentation and symbols are the content.
    if (inFence) {
      out.push(raw);
      continue;
    }
    // Horizontal rules become the blank line they were standing in for.
    if (/^\s*([-*_]\s*){3,}$/.test(line)) {
      out.push('');
      continue;
    }

    const heading = line.match(/^\s{0,3}#{1,6}\s+(.*)$/);
    if (heading) {
      if (out.length > 0 && out[out.length - 1] !== '') out.push('');
      out.push(stripInline(heading[1]).trim());
      continue;
    }

    const task = line.match(/^(\s*)[-*+]\s+\[([ xX])\]\s+(.*)$/);
    if (task) {
      out.push(`${task[1]}${task[2].toLowerCase() === 'x' ? '☑' : '☐'} ${stripInline(task[3])}`);
      continue;
    }

    const bullet = line.match(/^(\s*)[-*+]\s+(.*)$/);
    if (bullet) {
      out.push(`${bullet[1]}• ${stripInline(bullet[2])}`);
      continue;
    }

    const quote = line.match(/^\s*>\s?(.*)$/);
    if (quote) {
      out.push(stripInline(quote[1]));
      continue;
    }

    out.push(stripInline(line));
  }

  return out.join('\n').replace(/\n{3,}/g, '\n\n').trim();
}

/** Meeting title and date, as the first lines of the shared message. */
export function buildShareHeader(title: string, createdAt: string | undefined): string {
  const parts: string[] = [];
  const cleanTitle = (title || '').trim();
  if (cleanTitle) parts.push(cleanTitle);

  if (createdAt) {
    const date = new Date(createdAt);
    if (!Number.isNaN(date.getTime())) {
      parts.push(
        date.toLocaleString(undefined, {
          year: 'numeric',
          month: 'long',
          day: 'numeric',
          hour: '2-digit',
          minute: '2-digit',
        }),
      );
    }
  }
  return parts.join('\n');
}

/**
 * The two halves of a Telegram share.
 *
 * `draft` is the short text the deep link can safely carry, and lands in the message box
 * of whichever chat the user picks: the meeting's title and date, plus an optional
 * `draftPlaceholder` line reminding the user to paste. `body` is the summary itself, which
 * goes to the clipboard and replaces that placeholder.
 *
 * `full` is the finished message — header and body, and deliberately *not* the placeholder,
 * since it is also what gets written to the `.md` file.
 */
export function buildTelegramShareParts(opts: {
  title: string;
  createdAt?: string;
  markdown: string;
  draftPlaceholder?: string;
}): { draft: string; body: string; full: string } {
  let body = markdownToTelegramText(opts.markdown);
  const header = buildShareHeader(opts.title, opts.createdAt);

  // Summaries often open with the meeting title as their first heading; the draft already
  // carries it, so drop the repeat.
  if (header) {
    const [firstLine, ...rest] = body.split('\n');
    if (firstLine.trim() && firstLine.trim() === (opts.title || '').trim()) {
      body = rest.join('\n').replace(/^\n+/, '');
    }
  }

  // The placeholder is part of the draft's budget, so a runaway title cannot push it out
  // of the link. With no header there is nothing to prefill but the reminder itself.
  const placeholder = (opts.draftPlaceholder || '').trim();
  const reserved = placeholder ? placeholder.length + 2 : 0;
  const { text: fittedHeader } = fitToBudget(
    header,
    Math.max(0, TELEGRAM_DRAFT_BUDGET - reserved),
  );

  const draft = [fittedHeader, placeholder].filter(Boolean).join('\n\n');
  const full = fittedHeader && body ? `${fittedHeader}\n\n${body}` : fittedHeader || body;
  return { draft, body, full };
}

/**
 * Cut `text` to `budget` characters including `suffix`, preferring a paragraph, then a
 * line, then a word boundary — anything but a cut mid-word. Counting in UTF-16 units
 * (`String.length`) rather than code points only ever over-counts, which keeps the
 * result inside the character limit the Rust side enforces.
 */
export function fitToBudget(
  text: string,
  budget: number,
  suffix = '',
): { text: string; truncated: boolean } {
  if (text.length <= budget) return { text, truncated: false };

  const room = Math.max(0, budget - suffix.length);
  const head = text.slice(0, room);
  const floor = room * 0.5;
  const paragraph = head.lastIndexOf('\n\n');
  const line = head.lastIndexOf('\n');
  const space = head.lastIndexOf(' ');

  let cut = room;
  if (paragraph > floor) cut = paragraph;
  else if (line > floor) cut = line;
  else if (space > floor) cut = space;

  return { text: head.slice(0, cut).replace(/\s+$/, '') + suffix, truncated: true };
}
