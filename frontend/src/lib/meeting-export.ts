import { imageDataToDataUrl } from '@/lib/transcript-annotation-input';
import type { Transcript, TranscriptAnnotation } from '@/types';

export interface MeetingExportInput {
  title: string;
  date: string;
  summaryMarkdown?: string | null;
  segments: Transcript[];
  annotations: ExportAnnotation[];
}

export type ExportAnnotation = TranscriptAnnotation & {
  /** Resolved image data from an annotation file, ready for Markdown/PDF output. */
  imageDataUrl?: string;
};

export type MeetingExportEvent =
  | { kind: 'segment'; time: number; segment: Transcript }
  | { kind: 'annotation'; time: number; annotation: ExportAnnotation };

function timestamp(seconds: number | undefined): string {
  if (seconds === undefined || !Number.isFinite(seconds)) return '[--:--]';
  const totalSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(totalSeconds / 60);
  const remainingSeconds = totalSeconds % 60;
  return `[${minutes.toString().padStart(2, '0')}:${remainingSeconds.toString().padStart(2, '0')}]`;
}

function eventTime(event: MeetingExportEvent): number {
  return event.kind === 'segment'
    ? event.segment.audio_start_time ?? Number.POSITIVE_INFINITY
    : event.annotation.anchorTime;
}

/** Merge transcript segments and annotations in recording-relative time order. */
export function buildExportTimeline(
  segments: Transcript[],
  annotations: ExportAnnotation[],
): MeetingExportEvent[] {
  const events: Array<MeetingExportEvent & { originalIndex: number }> = [
    ...segments.map((segment, originalIndex) => ({
      kind: 'segment' as const,
      time: segment.audio_start_time ?? Number.POSITIVE_INFINITY,
      segment,
      originalIndex,
    })),
    ...annotations.map((annotation, originalIndex) => ({
      kind: 'annotation' as const,
      time: annotation.anchorTime,
      annotation,
      originalIndex: segments.length + originalIndex,
    })),
  ];

  return events
    .sort((a, b) => {
      const timeDifference = eventTime(a) - eventTime(b);
      if (timeDifference !== 0) return timeDifference;
      // Annotations at a segment's timestamp appear before that segment's text.
      if (a.kind !== b.kind) return a.kind === 'annotation' ? -1 : 1;
      return a.originalIndex - b.originalIndex;
    })
    .map(event => event.kind === 'segment'
      ? { kind: event.kind, time: event.time, segment: event.segment }
      : { kind: event.kind, time: event.time, annotation: event.annotation });
}

function annotationImageDataUrl(annotation: ExportAnnotation): string | null {
  if (annotation.imageDataUrl?.startsWith('data:')) return annotation.imageDataUrl;
  if (annotation.imageData && annotation.imageData.length > 0) {
    return imageDataToDataUrl(annotation.imageData, annotation.imageMime || 'image/png');
  }
  return null;
}

function renderAnnotation(annotation: ExportAnnotation): string {
  const time = timestamp(annotation.anchorTime);
  if (annotation.type === 'image') {
    const imageDataUrl = annotationImageDataUrl(annotation);
    if (!imageDataUrl) return `- ${time} Image: ${annotation.text?.trim() || 'Image unavailable'}`;
    const caption = annotation.text?.trim() || 'Screenshot';
    return `![${caption} ${time}](${imageDataUrl})`;
  }

  const label = annotation.type === 'bookmark' ? 'Bookmark' : 'Note';
  const text = annotation.text?.trim();
  return `- ${time} ${label}${text ? `: ${text}` : ''}`;
}

function renderSegment(segment: Transcript): string {
  const source = segment.source && segment.source !== 'Audio' ? `${segment.source}: ` : '';
  return `${timestamp(segment.audio_start_time)} ${source}${segment.text}`;
}

/** Build the portable Markdown representation of a saved meeting. */
export function buildMeetingMarkdown({
  title,
  date,
  summaryMarkdown,
  segments,
  annotations,
}: MeetingExportInput): string {
  const parts = [`# ${title}`, `## Date: ${date}`];
  if (summaryMarkdown?.trim()) {
    parts.push(`## AI Summary\n\n${summaryMarkdown.trim()}`);
  }

  parts.push('## Transcript');
  parts.push(
    ...buildExportTimeline(segments, annotations).map(event =>
      event.kind === 'segment' ? renderSegment(event.segment) : renderAnnotation(event.annotation)
    ),
  );

  return `${parts.join('\n\n')}\n`;
}

/** Resolve the image source used by both Markdown and PDF exports. */
export function getExportImageDataUrl(annotation: ExportAnnotation): string | null {
  return annotationImageDataUrl(annotation);
}
