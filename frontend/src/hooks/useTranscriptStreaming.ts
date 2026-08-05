import { useState, useEffect, useRef } from 'react';
import { TranscriptSegmentData } from '@/types';

/**
 * Marks each newly recognised segment once so the deslop StreamingText
 * component can reveal it by words. Text slicing intentionally lives nowhere
 * in this hook: the animation component always receives the complete phrase.
 */
export function useTranscriptStreaming(
  segments: TranscriptSegmentData[],
  isRecording: boolean,
  enableStreaming: boolean
) {
  const [streamingSegmentId, setStreamingSegmentId] = useState<string | null>(null);
  const lastSegmentIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!isRecording || !enableStreaming || segments.length === 0) {
      setStreamingSegmentId(null);
      lastSegmentIdRef.current = null;
      return;
    }

    const latestSegment = segments[segments.length - 1];

    // Check if this is a new segment
    if (latestSegment.id !== lastSegmentIdRef.current) {
      lastSegmentIdRef.current = latestSegment.id;
      setStreamingSegmentId(latestSegment.id);
    }
  }, [segments, isRecording, enableStreaming]);

  /**
   * Get the display text for a segment, with streaming effect if applicable
   */
  const getDisplayText = (segment: TranscriptSegmentData): string => {
    return segment.text;
  };

  return {
    streamingSegmentId,
    getDisplayText,
  };
}
