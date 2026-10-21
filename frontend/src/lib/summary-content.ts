import type { MeetingSummary } from '@/types';
import { z } from 'zod';

const REASONING_MARKER = /<\/?think(?:ing)?(?:\s+[^>]*)?>/i;
const SummaryObject = z.record(z.unknown());

function parseSummaryObject(value: unknown) {
  const parsed = SummaryObject.safeParse(value);
  return parsed.success ? parsed.data : null;
}

function containsReasoningMarker(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some(containsReasoningMarker);
  }
  const object = parseSummaryObject(value);
  if (!object) {
    return false;
  }

  return Object.entries(object).some(([key, child]) => {
    if (typeof child === 'string') {
      return ['markdown', 'text', 'content'].includes(key) && REASONING_MARKER.test(child);
    }
    return ['summary_json', 'children', 'content', 'blocks'].includes(key)
      && containsReasoningMarker(child);
  });
}

function hasBlockText(value: unknown): boolean {
  if (typeof value === 'string') {
    return value.trim().length > 0;
  }
  if (Array.isArray(value)) {
    return value.some(hasBlockText);
  }
  const object = parseSummaryObject(value);
  return object ? [object.text, object.content, object.children].some(hasBlockText) : false;
}

function hasLegacySectionText(value: unknown): boolean {
  const section = parseSummaryObject(value);
  return Array.isArray(section?.blocks) && section.blocks.some((block) => {
    const parsedBlock = parseSummaryObject(block);
    return typeof parsedBlock?.content === 'string' && parsedBlock.content.trim().length > 0;
  });
}

export function hasVisibleSummaryContent(value: unknown): value is MeetingSummary {
  const object = parseSummaryObject(value);
  if (!object || containsReasoningMarker(value)) {
    return false;
  }
  if ('markdown' in object) {
    return typeof object.markdown === 'string' && object.markdown.trim().length > 0;
  }
  if ('summary_json' in object) {
    return hasBlockText(object.summary_json);
  }
  return Object.entries(object)
    .filter(([key]) => key !== 'MeetingName' && key !== '_section_order')
    .some(([, section]) => hasLegacySectionText(section));
}

const SummaryMetadata = z.object({
  MeetingName: z.string().optional(),
  reasoning_stripped: z.boolean().optional(),
  normalization_fallback: z.boolean().optional(),
}).passthrough();

export function readSummaryMetadata(value: unknown) {
  const parsed = SummaryMetadata.safeParse(value);
  return parsed.success
    ? {
        meetingName: parsed.data.MeetingName ?? null,
        reasoningStripped: parsed.data.reasoning_stripped === true,
        normalizationFallback: parsed.data.normalization_fallback === true,
      }
    : {
        meetingName: null,
        reasoningStripped: false,
        normalizationFallback: false,
      };
}

export function parseSummaryContent(value: unknown): MeetingSummary | null {
  if (typeof value === 'string') {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  return hasVisibleSummaryContent(value) ? value : null;
}
