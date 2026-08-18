import { isUnresolvedSpeakerLabel, localizeSpeakerLabel, type SpeakerInfo } from '@/types';

const NON_NAMES = new Set(['ну', 'вот', 'сказали']);

function isResolvedName(value: string): boolean {
  const normalized = value.trim().toLocaleLowerCase('ru-RU');
  return !isUnresolvedSpeakerLabel(value) && !NON_NAMES.has(normalized);
}

export function normalizedSpeakerName(value: string): string {
  return value.trim().toLocaleLowerCase('ru-RU').replace(/ё/g, 'е');
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function escapeMarkdownLinkLabel(value: string): string {
  return value.replace(/[\[\]\\]/g, '\\$&');
}

interface SpeakerNameCandidate {
  speaker: SpeakerInfo;
  sourceLabel: string;
  currentLabel: string;
}

function speakerNameCandidates(
  speakers: readonly SpeakerInfo[],
  translate: (value: string) => string,
): SpeakerNameCandidate[] {
  const candidatesBySpeaker = speakers.flatMap((speaker) => {
    const currentLabel = localizeSpeakerLabel(speaker.display_name, translate)?.trim() ?? '';
    const sourceLabels = [speaker.display_name, ...(speaker.aliases ?? [])]
      .map((label) => localizeSpeakerLabel(label, translate)?.trim() ?? '')
      .filter(isResolvedName);
    const uniqueSourceLabels = sourceLabels.filter(
      (label, index, labels) => labels.findIndex(
        (candidate) => normalizedSpeakerName(candidate) === normalizedSpeakerName(label),
      ) === index,
    );
    return uniqueSourceLabels.map((sourceLabel): SpeakerNameCandidate => ({
      speaker,
      sourceLabel,
      currentLabel,
    }));
  });

  // Never assign an old label when it points to more than one speaker. A missing link is safer
  // than opening the rename dialog for the wrong person.
  const speakerIdsByLabel = new Map<string, Set<number>>();
  for (const candidate of candidatesBySpeaker) {
    const key = normalizedSpeakerName(candidate.sourceLabel);
    const ids = speakerIdsByLabel.get(key) ?? new Set<number>();
    ids.add(candidate.speaker.id);
    speakerIdsByLabel.set(key, ids);
  }

  return candidatesBySpeaker
    .filter((candidate) => speakerIdsByLabel.get(normalizedSpeakerName(candidate.sourceLabel))?.size === 1)
    .sort((left, right) => right.sourceLabel.length - left.sourceLabel.length);
}

function speakerLabelPattern(sourceLabel: string, anywhere: boolean): RegExp {
  const escaped = escapeRegExp(sourceLabel);
  const boundary = anywhere ? '(?<![\\p{L}\\p{N}_])' : '';
  const endBoundary = anywhere ? '(?![\\p{L}\\p{N}_])' : '(?=\\s|[,:;.!?—–-]|$)';
  return new RegExp(
    `(?:\\*\\*${escaped}\\*\\*|__${escaped}__|${boundary}${escaped}${endBoundary})`,
    'igu',
  );
}

function speakerLink(candidate: SpeakerNameCandidate): string {
  return `[${escapeMarkdownLinkLabel(candidate.currentLabel)}](#speaker-${candidate.speaker.id})`;
}

const MARKDOWN_LINK_PATTERN = /\[(?:\\.|[^\]\\\n])*\]\([^)\n]*\)/gu;
const SPEAKER_LINK_PATTERN = /\[(?:\\.|[^\]\\\n])*\]\(#speaker-(\d+)\)/gu;
const NESTED_SPEAKER_LINK_PATTERN = /\[(\[(?:\\.|[^\]\\\n])*\]\(#speaker-(\d+)\))\]\(#speaker-(\d+)\)/gu;

/**
 * Keep the renderer's private speaker links idempotent. A summary can already contain a
 * link from an earlier render or from an older saved version; wrapping its label again turns
 * into visible text such as `[[Name](#speaker-7)](#speaker-7)` when the link is rendered as
 * a rename button.
 */
function normalizeSpeakerLinks(
  markdown: string,
  candidates: readonly SpeakerNameCandidate[],
): string {
  const candidatesById = new Map(candidates.map((candidate) => [candidate.speaker.id, candidate]));
  let normalized = markdown;

  // Collapse the artifact produced by the previous non-idempotent renderer. A small bounded
  // loop also cleans up more than one historical nesting layer without touching user prose.
  for (let pass = 0; pass < 4; pass += 1) {
    const next = normalized.replace(
      NESTED_SPEAKER_LINK_PATTERN,
      (match, _innerLink, innerId: string, outerId: string) => {
        const candidate = candidatesById.get(Number(outerId)) ?? candidatesById.get(Number(innerId));
        return candidate ? speakerLink(candidate) : match;
      },
    );
    if (next === normalized) break;
    normalized = next;
  }

  return normalized.replace(SPEAKER_LINK_PATTERN, (match, speakerId: string) => {
    const candidate = candidatesById.get(Number(speakerId));
    return candidate ? speakerLink(candidate) : match;
  });
}

/** Replace speaker names only in plain Markdown text, never inside an existing link. */
function replaceOutsideMarkdownLinks(
  markdown: string,
  pattern: RegExp,
  replacement: string,
): string {
  let result = '';
  let cursor = 0;

  for (const match of markdown.matchAll(MARKDOWN_LINK_PATTERN)) {
    const start = match.index ?? 0;
    result += markdown.slice(cursor, start).replace(pattern, replacement);
    result += match[0];
    cursor = start + match[0].length;
  }

  return result + markdown.slice(cursor).replace(pattern, replacement);
}

/**
 * Resolve speaker names embedded in persisted agreement prose back to stable speaker ids.
 *
 * A summary may still say an old alias after the user renames a speaker. Matching the alias
 * lets us render the current display name and keep the rename control connected without
 * rewriting or regenerating the stored summary.
 */
export function linkAgreementSpeakers(
  markdown: string,
  speakers: readonly SpeakerInfo[],
  translate: (value: string) => string,
): string {
  const candidates = speakerNameCandidates(speakers, translate);

  return normalizeSpeakerLinks(markdown, candidates)
    .split('\n')
    .map((line) => {
      const listItem = line.match(/^(\s*(?:[-+*•]|\d+[.)])\s+)(.*)$/u);
      const marker = listItem?.[1] ?? '';
      const body = listItem?.[2] ?? line;
      for (const { speaker, sourceLabel, currentLabel } of candidates) {
        const match = body.match(new RegExp(
          `^(?:\\*\\*${escapeRegExp(sourceLabel)}\\*\\*|__${escapeRegExp(sourceLabel)}__|${escapeRegExp(sourceLabel)})(?=\\s|[,:;.!?—–-]|$)`,
          'iu',
        ));
        if (!match) continue;

        return `${marker}${speakerLink({ speaker, sourceLabel, currentLabel })}${body.slice(match[0].length)}`;
      }
      return line;
    })
    .join('\n');
}

/**
 * Link speaker names wherever they appear in the summary lead. The persisted summary keeps
 * the old prose after a rename; rendering aliases through the stable speaker id lets the UI
 * show the current name and keeps the click-to-rename affordance working without regenerating
 * the user's summary.
 */
export function linkSpeakerNames(
  markdown: string,
  speakers: readonly SpeakerInfo[],
  translate: (value: string) => string,
): string {
  const candidates = speakerNameCandidates(speakers, translate);

  return normalizeSpeakerLinks(markdown, candidates).split('\n').map((line) => {
    let linkedLine = line;
    for (const candidate of candidates) {
      linkedLine = replaceOutsideMarkdownLinks(
        linkedLine,
        speakerLabelPattern(candidate.sourceLabel, true),
        speakerLink(candidate),
      );
    }
    return linkedLine;
  }).join('\n');
}

/** Find a report role stored under either the current speaker name or one of its aliases. */
export function roleForSpeaker(
  speaker: SpeakerInfo,
  rolesByName: ReadonlyMap<string, string>,
): string | undefined {
  for (const label of [speaker.display_name, ...(speaker.aliases ?? [])]) {
    const role = rolesByName.get(normalizedSpeakerName(label));
    if (role) return role;
  }
  return undefined;
}
