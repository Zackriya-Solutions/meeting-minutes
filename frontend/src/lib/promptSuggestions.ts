import type { Transcript } from "@/types";
import { summaryToMarkdown } from "@/lib/summaryToMarkdown";

type AppLanguage = "ru" | "en";

interface ArchiveSuggestionSource {
  title: string;
  description?: string | null;
}

interface MeetingSuggestionSource {
  title: string;
  summary: unknown;
  transcripts: Transcript[];
}

const GENERIC_TITLES = [
  /^meeting(?:[\s_-]+\d|$)/i,
  /^встреча(?:[\s_-]+\d|$)/i,
  /^новая встреча$/i,
];

const GENERIC_SECTIONS = new Set([
  "summary",
  "overview",
  "main topics",
  "action items",
  "participants",
  "краткое содержание",
  "саммари",
  "о чём встреча",
  "о чем встреча",
  "основные темы",
  "договорённости",
  "договоренности",
  "задачи",
  "участники",
  "тайминг",
]);

function cleanText(value: string): string {
  return value
    .replace(/<!--[^]*?-->/g, " ")
    .replace(/\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/^\s*(?:#{1,6}|[-+*]|\d+[.)])\s+/gm, "")
    .replace(/[\n\r\t*_~`>|]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[.,:;!?—–-]+$/u, "");
}

function shorten(value: string, maxCharacters = 46): string {
  const clean = cleanText(value);
  if (Array.from(clean).length <= maxCharacters) return clean;

  const words = clean.split(/\s+/);
  const selected: string[] = [];
  for (const word of words) {
    const next = [...selected, word].join(" ");
    if (selected.length > 0 && Array.from(next).length > maxCharacters) break;
    selected.push(word);
  }
  return `${selected.join(" ").replace(/[.,:;!?—–-]+$/u, "")}…`;
}

function quote(value: string): string {
  return `«${value}»`;
}

function uniqueTopics(
  values: Array<string | null | undefined>,
  maxCharacters = 46,
): string[] {
  const seen = new Set<string>();
  return values.flatMap((value) => {
    const clean = value ? shorten(value, maxCharacters) : "";
    const key = clean.toLocaleLowerCase();
    if (!clean || seen.has(key)) return [];
    seen.add(key);
    return [clean];
  });
}

function uniqueSuggestions(values: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  return values.flatMap((value) => {
    const clean = value?.replace(/\s+/g, " ").trim() ?? "";
    const key = clean.toLocaleLowerCase();
    if (!clean || seen.has(key)) return [];
    seen.add(key);
    return [clean];
  });
}

function usefulTitle(title: string): string | null {
  const clean = shorten(title, 40);
  return clean && !GENERIC_TITLES.some((pattern) => pattern.test(clean)) ? clean : null;
}

function spreadAcrossArchive<T>(values: readonly T[], limit: number): T[] {
  if (values.length <= limit) return [...values];
  if (limit <= 1) return [values[0]];

  return Array.from({ length: limit }, (_, index) => {
    const sourceIndex = Math.round((index * (values.length - 1)) / (limit - 1));
    return values[sourceIndex];
  });
}

function extractMeetingTopics(summary: unknown, transcripts: Transcript[]): string[] {
  const markdown = summaryToMarkdown(summary);
  const summaryLines = markdown
    .split(/\n+/)
    .map(cleanText)
    .filter((line) => line.length >= 8 && !GENERIC_SECTIONS.has(line.toLocaleLowerCase()));

  if (summaryLines.length > 0) return uniqueTopics(summaryLines, 48).slice(0, 3);

  const transcriptLines = transcripts
    .map((transcript) => cleanText(transcript.text))
    .filter((line) => line.length >= 12);

  return uniqueTopics(spreadAcrossArchive(transcriptLines, 3), 48);
}

export function buildArchivePromptSuggestions(
  sources: readonly ArchiveSuggestionSource[],
  lang: AppLanguage,
): string[] {
  const archiveSample = spreadAcrossArchive(sources, 4);
  const titles = uniqueTopics(archiveSample.map((source) => usefulTitle(source.title)));
  const topics = uniqueTopics(sources.map((source) => source.description), 48).slice(0, 2);

  if (lang === "ru") {
    return uniqueSuggestions([
      topics[0] ? `Что решили про ${quote(topics[0])}?` : null,
      titles[0] ? `Что осталось сделать после ${quote(titles[0])}?` : null,
      titles[1] ? "Сравни последние встречи" : null,
      topics[1] ? `На каких встречах обсуждали ${quote(topics[1])}?` : null,
      "Какие решения повторяются?",
      "Что изменилось недавно?",
    ]);
  }

  return uniqueSuggestions([
    topics[0] ? `What was decided about ${quote(topics[0])}?` : null,
    titles[0] ? `What is still open after ${quote(titles[0])}?` : null,
    titles[1] ? `Compare ${quote(titles[0])} and ${quote(titles[1])}` : null,
    topics[1] ? `Where else was ${quote(topics[1])} discussed?` : null,
    "Which agreements recur across meetings?",
    "What changed across the latest meetings?",
  ]);
}

export function buildMeetingPromptSuggestions(
  source: MeetingSuggestionSource,
  lang: AppLanguage,
): string[] {
  const title = usefulTitle(source.title);
  const topics = extractMeetingTopics(source.summary, source.transcripts);

  if (lang === "ru") {
    return uniqueSuggestions([
      topics[0] ? `Что решили про ${quote(topics[0])}?` : null,
      topics[1] ? `Что делать дальше по ${quote(topics[1])}?` : null,
      title ? `Чем закончилась встреча ${quote(title)}?` : null,
      "Кто что делает дальше?",
      "Что осталось нерешённым?",
      "Собери итоги",
    ]);
  }

  return uniqueSuggestions([
    topics[0] ? `What was decided about ${quote(topics[0])}?` : null,
    topics[1] ? `What are the next steps for ${quote(topics[1])}?` : null,
    title ? `What was the outcome of ${quote(title)}?` : null,
    "Who owns the next steps?",
    "Which risks and open questions remain?",
    "Summarize the key agreements",
  ]);
}
