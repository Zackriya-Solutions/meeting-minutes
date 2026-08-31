import { TranscriptAnnotation, TranscriptSegmentData } from '@/types';

export type TranscriptTimelineItem =
  | { kind: 'segment'; key: string; time: number; segment: TranscriptSegmentData }
  | { kind: 'annotation'; key: string; time: number; annotation: TranscriptAnnotation };

function itemTime(item: TranscriptTimelineItem): number {
  return item.kind === 'segment' ? item.time : item.annotation.anchorTime;
}

/** Merge transcript segments and manual annotations in stable chronological order. */
export function mergeTranscriptTimeline(
  segments: TranscriptSegmentData[],
  annotations: TranscriptAnnotation[],
): TranscriptTimelineItem[] {
  const items: TranscriptTimelineItem[] = [
    ...segments.map((segment, index) => ({
      kind: 'segment' as const,
      key: `segment-${segment.id}-${index}`,
      time: segment.timestamp,
      segment,
    })),
    ...annotations.map((annotation, index) => ({
      kind: 'annotation' as const,
      key: `annotation-${annotation.id}-${index}`,
      time: annotation.anchorTime,
      annotation,
    })),
  ];

  return items.sort((a, b) => itemTime(a) - itemTime(b));
}
