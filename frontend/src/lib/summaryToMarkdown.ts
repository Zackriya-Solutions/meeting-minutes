/**
 * Best-effort conversion of a persisted meeting summary (a markdown blob, BlockNote
 * JSON, or a legacy section map) into Markdown, plus a TL;DR/details split for the
 * 3a summary layout (a prominent lead + a collapsible "Подробнее" section).
 *
 * Real summaries are markdown where section titles are bold lines (**Краткое
 * содержание**, **Ключевые решения**, **Задачи** …). We normalize those to real
 * Markdown headings so they render as sections and the split can use them as
 * boundaries. Anything unknown degrades to an empty string rather than throwing.
 */

function inlineText(content: any): string {
  if (content == null) return '';
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .map((run) => (typeof run === 'string' ? run : run?.text ?? run?.content ?? ''))
      .join('');
  }
  if (typeof content === 'object') return content.text ?? '';
  return '';
}

function blocksToMarkdown(blocks: any[]): string {
  const lines: string[] = [];
  for (const block of blocks) {
    const text = inlineText(block?.content).trim();
    const type = block?.type;
    if (type === 'heading') {
      const level = Math.min(Math.max(Number(block?.props?.level) || 2, 1), 4);
      lines.push('', `${'#'.repeat(level)} ${text}`);
    } else if (type === 'bulletListItem') {
      lines.push(`- ${text}`);
    } else if (type === 'numberedListItem') {
      lines.push(`1. ${text}`);
    } else if (type === 'checkListItem') {
      lines.push(`- [${block?.props?.checked ? 'x' : ' '}] ${text}`);
    } else if (text) {
      lines.push('', text);
    }
  }
  return lines.join('\n').trim();
}

function sectionsToMarkdown(summary: Record<string, any>): string {
  const order: string[] = Array.isArray(summary._section_order)
    ? summary._section_order
    : Object.keys(summary);
  const parts: string[] = [];
  for (const key of order) {
    if (key === 'MeetingName' || key === '_section_order') continue;
    const section = summary[key];
    if (!section || typeof section !== 'object' || !Array.isArray(section.blocks)) continue;
    const title = typeof section.title === 'string' ? section.title : key;
    const body = section.blocks
      .map((b: any) => {
        const content = inlineText(b?.content).trim();
        return content ? `- ${content}` : '';
      })
      .filter(Boolean)
      .join('\n');
    parts.push(`## ${title}${body ? `\n${body}` : ''}`);
  }
  return parts.join('\n\n').trim();
}

/** Turn bold-only lines (**Section**) into real Markdown headings. */
function normalizeHeadings(md: string): string {
  return md
    .split('\n')
    .map((line) => {
      const trimmed = line.trim();
      const m = trimmed.match(/^\*\*(.+?)\*\*:?$/);
      return m ? `## ${m[1].trim()}` : line;
    })
    .join('\n');
}

/** Convert any persisted summary shape to Markdown (empty string if unknown). */
export function summaryToMarkdown(summary: any): string {
  try {
    if (!summary) return '';
    let md = '';
    if (typeof summary === 'string') md = summary;
    else if (typeof summary.markdown === 'string' && summary.markdown.trim()) md = summary.markdown;
    else if (Array.isArray(summary.summary_json)) md = blocksToMarkdown(summary.summary_json);
    else if (typeof summary === 'object') md = sectionsToMarkdown(summary);
    return normalizeHeadings(md.trim()).trim();
  } catch {
    return '';
  }
}

/**
 * Split summary markdown into a TL;DR lead (the body of the first section) and the
 * remaining details (subsequent sections: key decisions / tasks). Either may be
 * empty. Assumes headings have been normalized by summaryToMarkdown.
 */
export function splitSummaryLead(markdown: string): { lead: string; details: string } {
  const md = (markdown || '').trim();
  if (!md) return { lead: '', details: '' };

  const blocks = md.split(/\n{2,}/).map((s) => s.trim()).filter(Boolean);
  const isHeading = (b: string) => /^#{1,6}\s/.test(b);
  const isListOrTable = (b: string) => /^([-*]\s|\d+\.\s|\||- \[)/.test(b);
  const headingIdxs = blocks.map((b, i) => (isHeading(b) ? i : -1)).filter((i) => i >= 0);

  if (headingIdxs.length === 0) {
    const leadIdx = blocks.findIndex((b) => !isListOrTable(b));
    if (leadIdx === -1) return { lead: '', details: md };
    return { lead: blocks[leadIdx], details: blocks.filter((_, i) => i !== leadIdx).join('\n\n') };
  }

  const first = headingIdxs[0];
  const second = headingIdxs[1] ?? blocks.length;
  const overview = blocks
    .slice(first + 1, second)
    .filter((b) => !isHeading(b) && !isListOrTable(b));
  const lead = overview.join('\n\n').trim();
  const details = blocks.slice(second).join('\n\n').trim();

  if (!lead) return { lead: '', details: blocks.slice(first).join('\n\n').trim() };
  return { lead, details };
}
