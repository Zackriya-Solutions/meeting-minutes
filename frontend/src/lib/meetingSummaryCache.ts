import { invoke } from '@tauri-apps/api/core';
import type { Summary } from '@/types';

const MAX_CACHED_SUMMARIES = 50;

const summaryCache = new Map<string, Summary>();
const pendingSummaryReads = new Map<string, Promise<Summary | null>>();

export function parsePersistedSummary(rawData: unknown): Summary | null {
  let parsedData: any = rawData;
  if (typeof parsedData === 'string') {
    try {
      parsedData = JSON.parse(parsedData);
    } catch {
      return null;
    }
  }
  if (!parsedData || typeof parsedData !== 'object') return null;

  // Current formats must retain their generation/freshness metadata.
  if (parsedData.summary_json || parsedData.markdown) return parsedData as Summary;

  // Legacy section format.
  const { MeetingName: _meetingName, _section_order, ...restSummaryData } = parsedData;
  const formattedSummary: Summary = {};
  const sectionKeys = Array.isArray(_section_order)
    ? _section_order
    : Object.keys(restSummaryData);

  for (const key of sectionKeys) {
    const section = restSummaryData[key];
    if (!section || typeof section !== 'object' || !('title' in section) || !('blocks' in section)) {
      continue;
    }
    const blocks = Array.isArray(section.blocks) ? section.blocks : [];
    formattedSummary[key] = {
      title: typeof section.title === 'string' ? section.title : key,
      blocks: blocks.map((block: any) => ({
        ...block,
        color: 'default',
        content: block?.content?.trim() || '',
      })),
    };
  }

  return Object.keys(formattedSummary).length > 0 ? formattedSummary : null;
}

export function readCachedMeetingSummary(meetingId: string): Summary | null {
  const summary = summaryCache.get(meetingId) ?? null;
  if (!summary) return null;

  // Refresh insertion order so the bounded map behaves as a small LRU cache.
  summaryCache.delete(meetingId);
  summaryCache.set(meetingId, summary);
  return summary;
}

export function cacheMeetingSummary(meetingId: string, summary: Summary | null): void {
  summaryCache.delete(meetingId);
  if (!summary) return;

  summaryCache.set(meetingId, summary);
  while (summaryCache.size > MAX_CACHED_SUMMARIES) {
    const oldestMeetingId = summaryCache.keys().next().value as string | undefined;
    if (!oldestMeetingId) break;
    summaryCache.delete(oldestMeetingId);
  }
}

export async function prefetchMeetingSummary(meetingId: string): Promise<Summary | null> {
  const cached = readCachedMeetingSummary(meetingId);
  if (cached) return cached;

  const pending = pendingSummaryReads.get(meetingId);
  if (pending) return pending;

  const request = invoke<{ data?: unknown }>('api_get_summary', { meetingId })
    .then((response) => {
      const summary = response?.data == null ? null : parsePersistedSummary(response.data);
      if (summary) cacheMeetingSummary(meetingId, summary);
      return summary;
    })
    .catch((error) => {
      // Prefetch is an optimization. The meeting screen performs the authoritative
      // read and owns user-facing error handling if this speculative request fails.
      console.debug(`Could not prefetch summary for ${meetingId}:`, error);
      return null;
    })
    .finally(() => {
      pendingSummaryReads.delete(meetingId);
    });

  pendingSummaryReads.set(meetingId, request);
  return request;
}
