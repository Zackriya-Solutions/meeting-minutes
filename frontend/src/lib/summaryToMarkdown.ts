/**
 * Best-effort conversion of a persisted meeting summary (BlockNote JSON, legacy
 * section map, or a plain markdown blob) into Markdown, plus a TL;DR/details split
 * for the 3a summary layout (a prominent lead + a collapsible "Подробнее" section).
 *
 * The real summary is free-form generated content, so this is intentionally
 * defensive: anything it cannot understand degrades to an empty string rather than
 * throwing, and callers fall back to showing nothing / the generate state.
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

/** Convert any persisted summary shape to Markdown (empty string if unknown). */
export function summaryToMarkdown(summary: any): string {
  try {
    if (!summary) return '';
    if (typeof summary === 'string') return summary.trim();
    if (typeof summary.markdown === 'string' && summary.markdown.trim()) return summary.markdown.trim();
    if (Array.isArray(summary.summary_json)) return blocksToMarkdown(summary.summary_json);
    if (typeof summary === 'object') return sectionsToMarkdown(summary);
    return '';
  } catch {
    return '';
  }
}

/**
 * Split summary markdown into a TL;DR lead (the first prose paragraph) and the
 * remaining details (key decisions / tasks). Either may be empty.
 */
export function splitSummaryLead(markdown: string): { lead: string; details: string } {
  const md = (markdown || '').trim();
  if (!md) return { lead: '', details: '' };
  const paragraphs = md.split(/\n{2,}/).map((p) => p.trim()).filter(Boolean);
  const isProse = (p: string) => !/^(#{1,6}\s|[-*]\s|\d+\.\s|- \[)/.test(p);
  const leadIdx = paragraphs.findIndex(isProse);
  if (leadIdx === -1) return { lead: '', details: md };
  const lead = paragraphs[leadIdx];
  const details = paragraphs.filter((_, i) => i !== leadIdx).join('\n\n');
  return { lead, details };
}
