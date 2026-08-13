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
  const candidates = candidatesBySpeaker
    .filter((candidate) => speakerIdsByLabel.get(normalizedSpeakerName(candidate.sourceLabel))?.size === 1)
    .sort((left, right) => right.sourceLabel.length - left.sourceLabel.length);

  return markdown
    .split('\n')
    .map((line) => {
      const listItem = line.match(/^(\s*(?:[-+*•]|\d+[.)])\s+)(.*)$/u);
      if (!listItem) return line;

      const [, marker, body] = listItem;
      for (const { speaker, sourceLabel, currentLabel } of candidates) {
        const match = body.match(new RegExp(
          `^(?:\\*\\*${escapeRegExp(sourceLabel)}\\*\\*|__${escapeRegExp(sourceLabel)}__|${escapeRegExp(sourceLabel)})(?=\\s|[,:;.!?—–-]|$)`,
          'iu',
        ));
        if (!match) continue;

        return `${marker}[${escapeMarkdownLinkLabel(currentLabel)}](#speaker-${speaker.id})${body.slice(match[0].length)}`;
      }
      return line;
    })
    .join('\n');
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
