import { replaceAutomaticSpeakerLabels } from '@/types';

/**
 * Best-effort conversion of a persisted meeting summary (a markdown blob, BlockNote
 * JSON, or a legacy section map) into Markdown, plus the read-first overview and
 * bounded agreements used by the meeting drawer.
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
    return replaceAutomaticSpeakerLabels(normalizeHeadings(md.trim()).trim());
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

  const lines = md.split('\n');
  const headings = lines.flatMap((line, index) => {
    const match = line.trim().match(/^(#{1,6})\s+(.+)$/);
    return match ? [{ index, level: match[1].length, title: match[2].trim() }] : [];
  });

  if (headings.length === 0) {
    const blocks = md.split(/\n{2,}/).map((block) => block.trim()).filter(Boolean);
    return { lead: blocks[0] ?? '', details: blocks.slice(1).join('\n\n') };
  }

  const summaryHeading = headings.find(({ title }) => {
    const normalized = normalizeSectionTitle(title);
    return normalized === 'summary'
      || normalized === 'overview'
      || normalized === 'краткое содержание'
      || normalized === 'о чем встреча'
      || normalized === 'саммари';
  }) ?? headings.find(({ level }) => level > 1) ?? headings[0];

  const nextHeading = headings.find(({ index }) => index > summaryHeading.index);
  const sectionEnd = nextHeading?.index ?? lines.length;
  const lead = lines.slice(summaryHeading.index + 1, sectionEnd).join('\n').trim();
  const details = nextHeading ? lines.slice(nextHeading.index).join('\n').trim() : '';

  return { lead, details };
}

interface MarkdownSection {
  title: string;
  body: string;
}

function normalizeSectionTitle(value: string): string {
  return value
    .toLocaleLowerCase('ru-RU')
    .replace(/ё/g, 'е')
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
}

function markdownSections(markdown: string): MarkdownSection[] {
  const sections: MarkdownSection[] = [];
  let active: MarkdownSection | null = null;

  for (const line of markdown.split('\n')) {
    const heading = line.trim().match(/^#{1,6}\s+(.+)$/);
    if (heading) {
      active = { title: heading[1].trim(), body: '' };
      sections.push(active);
      continue;
    }
    if (active) active.body += `${active.body ? '\n' : ''}${line}`;
  }

  return sections;
}

function isEmptyAgreement(value: string): boolean {
  const normalized = normalizeSectionTitle(value);
  return !normalized
    || /^(нет|не зафиксировано|не было|отсутствуют|none|not specified|no agreements|no decisions|no action items)$/.test(normalized)
    || ((normalized.includes('договоренност') || normalized.includes('решени'))
      && (normalized.includes('не зафиксирован') || normalized.includes('отсутству')))
    || (/^no\s/.test(normalized) && /(agreement|decision|commitment|action item)/.test(normalized));
}

function tableCells(line: string): string[] {
  return line
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim().replace(/^\*\*(.*?)\*\*$/, '$1'));
}

function sectionItems(section: MarkdownSection, kind: 'participant' | 'agreement' | 'decision' | 'action'): string[] {
  const items: string[] = [];
  const lines = section.body.split('\n').map((line) => line.trim()).filter(Boolean);

  for (const line of lines) {
    const listItem = line.match(/^[-+*]\s+(?:\[[ xX]\]\s+)?(.+)$/)?.[1];
    if (listItem) {
      if (!isEmptyAgreement(listItem)) items.push(listItem.trim());
      continue;
    }

    if (line.startsWith('|')) {
      if (/^\|?\s*:?-{3,}/.test(line)) continue;
      const cells = tableCells(line);
      const normalizedCells = cells.map(normalizeSectionTitle);
      const isHeader = normalizedCells.some((cell) =>
        ['owner', 'task', 'due', 'decision', 'ответственный', 'задача', 'срок', 'решение'].includes(cell),
      );
      if (isHeader) continue;

      const item = kind === 'action' && cells.length > 1 ? cells[1] : cells[0];
      if (item && !isEmptyAgreement(item)) items.push(item);
      continue;
    }

    if (!isEmptyAgreement(line)) items.push(line);
  }

  return items;
}

function uniqueMarkdownItems(items: string[]): string[] {
  return items.filter((item, index) => {
    const normalized = normalizeSectionTitle(item);
    return normalized && items.findIndex((candidate) => normalizeSectionTitle(candidate) === normalized) === index;
  });
}

function uniqueMarkdownList(items: string[], singleItemAsParagraph = false): string {
  const uniqueItems = uniqueMarkdownItems(items);
  if (singleItemAsParagraph && uniqueItems.length === 1) return uniqueItems[0];
  return uniqueItems.map((item) => `- ${item}`).join('\n');
}

/** Return the explicitly identified speakers from the generated attendee section. */
export function extractSummaryParticipants(
  markdown: string,
  localizeLabel?: (label: string) => string,
): string {
  if (!markdown.trim()) return '';

  const participants = markdownSections(markdown)
    .filter(({ title }) => {
      const normalized = normalizeSectionTitle(title);
      return normalized.includes('участник')
        || normalized.includes('attendee')
        || normalized.includes('participant')
        || normalized === 'attendance';
    })
    .flatMap((section) => sectionItems(section, 'participant'));

  const localizedParticipants = localizeLabel
    ? participants.map(localizeLabel)
    : participants;

  return uniqueMarkdownList(localizedParticipants);
}

/**
 * Return a short, display-ready list of explicit agreements. New summaries contain a
 * dedicated section; older summaries fall back to their decisions and assigned actions.
 */
export function extractSummaryAgreements(markdown: string): string {
  if (!markdown.trim()) return '';

  const sections = markdownSections(markdown);
  const classified = sections.map((section) => {
    const title = normalizeSectionTitle(section.title);
    const agreement = title.includes('договоренност') || title.includes('agreement') || title.includes('commitment');
    const decision = title.includes('решени') || title.includes('decision');
    const action = title.includes('задач')
      || title.includes('action item')
      || title.includes('следующие шаги')
      || title.includes('next steps')
      || title.includes('согласованные результаты')
      || title.includes('agreed deliverables');
    return { section, agreement, decision, action };
  });

  const explicit = classified
    .filter(({ agreement }) => agreement)
    .flatMap(({ section }) => sectionItems(section, 'agreement'));
  if (explicit.length > 0) return uniqueMarkdownList(explicit, true);

  const decisions = classified
    .filter(({ decision }) => decision)
    .flatMap(({ section }) => sectionItems(section, 'decision'));
  const actions = classified
    .filter(({ action }) => action)
    .flatMap(({ section }) => sectionItems(section, 'action'));
  const combined: string[] = [];
  for (let index = 0; index < Math.max(decisions.length, actions.length); index += 1) {
    if (decisions[index]) combined.push(decisions[index]);
    if (actions[index]) combined.push(actions[index]);
  }

  return uniqueMarkdownList(combined, true);
}

export interface SummaryAgreementRow {
  who: string;
  what: string;
  when: string;
}

const UNKNOWN_AGREEMENT_FIELD = /^(?:-|—|unknown|not stated|not specified|none|n\/a|неизвестно|не указан(?:о|а)?|не определен(?:о|а)?|не распознан(?:о|а)?|без срока)$/i;

function cleanAgreementField(value: string | undefined): string {
  const cleaned = (value ?? '')
    .trim()
    .replace(/^\*\*(.*?)\*\*$/, '$1')
    .replace(/<br\s*\/?\s*>/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return UNKNOWN_AGREEMENT_FIELD.test(cleaned) ? '' : cleaned;
}

function agreementColumnIndex(headers: string[], aliases: string[]): number {
  return headers.findIndex((header) => aliases.some((alias) => header === alias || header.includes(alias)));
}

function agreementRowsFromSection(
  section: MarkdownSection,
  kind: 'agreement' | 'decision' | 'action',
): SummaryAgreementRow[] {
  const lines = section.body.split('\n').map((line) => line.trim()).filter(Boolean);
  const tableLines = lines.filter((line) => line.startsWith('|'));
  const rows: SummaryAgreementRow[] = [];

  if (tableLines.length >= 2) {
    const headerLine = tableLines.find((line) => !/^\|?\s*:?-{3,}/.test(line));
    if (headerLine) {
      const headerIndex = tableLines.indexOf(headerLine);
      const headers = tableCells(headerLine).map(normalizeSectionTitle);
      const whoIndex = agreementColumnIndex(headers, [
        'who', 'owner', 'responsible', 'assignee', 'кто', 'ответственный', 'исполнитель',
      ]);
      const whatIndex = agreementColumnIndex(headers, [
        'what', 'task', 'action', 'agreement', 'decision', 'commitment',
        'что', 'задача', 'действие', 'договоренност', 'решение',
      ]);
      const whenIndex = agreementColumnIndex(headers, [
        'when', 'due', 'deadline', 'когда', 'срок', 'дедлайн',
      ]);

      tableLines.slice(headerIndex + 1).forEach((line) => {
        if (/^\|?\s*:?-{3,}/.test(line)) return;
        const cells = tableCells(line);
        const fallbackWhatIndex = kind === 'action' && cells.length > 1 ? 1 : 0;
        const what = cleanAgreementField(cells[whatIndex >= 0 ? whatIndex : fallbackWhatIndex]);
        if (!what || isEmptyAgreement(what)) return;
        rows.push({
          who: cleanAgreementField(cells[whoIndex >= 0 ? whoIndex : -1]),
          what,
          when: cleanAgreementField(cells[whenIndex >= 0 ? whenIndex : -1]),
        });
      });
    }
  }

  lines.forEach((line) => {
    if (line.startsWith('|')) return;
    const value = line.match(/^[-+*]\s+(?:\[[ xX]\]\s+)?(.+)$/)?.[1] ?? line;
    const what = cleanAgreementField(value);
    if (!what || isEmptyAgreement(what)) return;
    rows.push({ who: '', what, when: '' });
  });

  return rows;
}

/**
 * Return display-ready agreement rows. Structured action-item tables provide the
 * owner and due date; legacy bullet summaries remain valid with empty metadata.
 * Rows with a recognized owner are kept first without changing their relative order.
 */
export function extractSummaryAgreementRows(markdown: string): SummaryAgreementRow[] {
  if (!markdown.trim()) return [];

  const sections = markdownSections(markdown);
  const classified = sections.map((section) => {
    const title = normalizeSectionTitle(section.title);
    return {
      section,
      agreement: title.includes('договоренност') || title.includes('agreement') || title.includes('commitment'),
      decision: title.includes('решени') || title.includes('decision'),
      action: title.includes('задач')
        || title.includes('action item')
        || title.includes('следующие шаги')
        || title.includes('next steps')
        || title.includes('согласованные результаты')
        || title.includes('agreed deliverables'),
    };
  });

  const agreements = classified
    .filter(({ agreement }) => agreement)
    .flatMap(({ section }) => agreementRowsFromSection(section, 'agreement'));
  const decisions = classified
    .filter(({ decision }) => decision)
    .flatMap(({ section }) => agreementRowsFromSection(section, 'decision'));
  const actions = classified
    .filter(({ action }) => action)
    .flatMap(({ section }) => agreementRowsFromSection(section, 'action'));

  const combined = [...actions, ...(agreements.length > 0 ? agreements : decisions)];
  const unique: SummaryAgreementRow[] = [];
  combined.forEach((row) => {
    const normalizedWhat = normalizeSectionTitle(row.what);
    const existingIndex = unique.findIndex((candidate) => normalizeSectionTitle(candidate.what) === normalizedWhat);
    if (existingIndex < 0) {
      unique.push(row);
      return;
    }
    const existing = unique[existingIndex];
    if ((!existing.who && row.who) || (!existing.when && row.when)) {
      unique[existingIndex] = {
        who: existing.who || row.who,
        what: existing.what,
        when: existing.when || row.when,
      };
    }
  });

  return unique
    .map((row, index) => ({ row, index }))
    .sort((left, right) => Number(Boolean(right.row.who)) - Number(Boolean(left.row.who)) || left.index - right.index)
    .map(({ row }) => row);
}

/** Return the generated assessment of whether the meeting stayed on schedule. */
export function extractSummaryTiming(markdown: string): string {
  if (!markdown.trim()) return '';

  const timing = markdownSections(markdown).find(({ title }) => {
    const normalized = normalizeSectionTitle(title);
    return normalized === 'тайминг'
      || normalized === 'timing'
      || normalized === 'schedule'
      || normalized === 'time management';
  });

  return timing?.body.trim() ?? '';
}
