import { describe, expect, test } from 'bun:test';
import {
  classifySegment,
  formatTimecode,
  generateTimeline,
  summarizeSegmentText,
  type TimelineSegment,
} from '../../src/lib/meeting-timeline';

/** Builds a segment with sensible defaults for terser test data. */
function segment(partial: Partial<TimelineSegment> & { id: string; startTime: number }): TimelineSegment {
  return {
    endTime: partial.startTime + 3,
    text: 'Some discussion about the project.',
    ...partial,
  };
}

describe('formatTimecode', () => {
  test('formats sub-hour durations as M:SS', () => {
    expect(formatTimecode(0)).toBe('0:00');
    expect(formatTimecode(5)).toBe('0:05');
    expect(formatTimecode(65)).toBe('1:05');
    expect(formatTimecode(600)).toBe('10:00');
  });

  test('formats hour-plus durations as H:MM:SS', () => {
    expect(formatTimecode(3600)).toBe('1:00:00');
    expect(formatTimecode(3661)).toBe('1:01:01');
  });

  test('clamps negative and non-finite input to 0:00', () => {
    expect(formatTimecode(-10)).toBe('0:00');
    expect(formatTimecode(Number.NaN)).toBe('0:00');
    expect(formatTimecode(Number.POSITIVE_INFINITY)).toBe('0:00');
  });

  test('truncates fractional seconds toward zero', () => {
    expect(formatTimecode(59.9)).toBe('0:59');
  });
});

describe('summarizeSegmentText', () => {
  test('collapses whitespace and strips trailing punctuation', () => {
    expect(summarizeSegmentText('  Hello    world.  ')).toBe('Hello world');
  });

  test('cuts at the first sentence boundary when short enough', () => {
    expect(summarizeSegmentText('Quick note. Then a much longer follow-up sentence here.')).toBe(
      'Quick note',
    );
  });

  test('truncates long single sentences on a word boundary with an ellipsis', () => {
    const label = summarizeSegmentText(
      'This is a considerably long sentence that keeps going well beyond the limit',
      30,
    );
    expect(label.endsWith('…')).toBe(true);
    expect(label.length).toBeLessThanOrEqual(31);
    expect(label).not.toContain('  ');
  });

  test('falls back to a generic label for empty text', () => {
    expect(summarizeSegmentText('    ')).toBe('Discussion');
  });
});

describe('classifySegment', () => {
  test('detects action items via keyword phrases', () => {
    expect(classifySegment("Let's finalize the budget by Friday")).toBe('action');
    expect(classifySegment('I will send the report tomorrow')).toBe('action');
    expect(classifySegment('Action item: update the docs')).toBe('action');
    expect(classifySegment('Next steps are clear')).toBe('action');
  });

  test('detects questions when no action signal is present', () => {
    expect(classifySegment('What do you think about the design?')).toBe('question');
  });

  test('prefers action over question when both are present', () => {
    expect(classifySegment("Can you make sure it's done by tomorrow?")).toBe('action');
  });

  test('returns null for plain statements and empty text', () => {
    expect(classifySegment('The weather was nice today')).toBeNull();
    expect(classifySegment('   ')).toBeNull();
  });
});

describe('generateTimeline', () => {
  test('returns an empty timeline when there are no segments', () => {
    expect(generateTimeline([])).toEqual([]);
  });

  test('drops invalid segments (negative, non-finite, or empty text)', () => {
    const markers = generateTimeline([
      segment({ id: 'a', startTime: -1, text: 'negative start' }),
      segment({ id: 'b', startTime: Number.NaN, text: 'nan start' }),
      segment({ id: 'c', startTime: 10, text: '   ' }),
    ]);
    expect(markers).toEqual([]);
  });

  test('produces a single chapter marker for one segment', () => {
    const markers = generateTimeline([segment({ id: 'only', startTime: 0, text: 'Welcome everyone' })]);
    expect(markers).toHaveLength(1);
    expect(markers[0]).toMatchObject({
      kind: 'chapter',
      time: 0,
      segmentId: 'only',
      segmentIndex: 0,
      label: 'Welcome everyone',
    });
  });

  test('returns markers in chronological order', () => {
    const segments = Array.from({ length: 30 }, (_, i) =>
      segment({ id: `s${i}`, startTime: i * 20, text: `Topic number ${i}` }),
    );
    const markers = generateTimeline(segments);
    const times = markers.map((m) => m.time);
    const sorted = [...times].sort((a, b) => a - b);
    expect(times).toEqual(sorted);
  });

  test('preserves the original segment index for scroll-to navigation', () => {
    const segments = [
      segment({ id: 's0', startTime: 0, text: 'Intro' }),
      segment({ id: 's1', startTime: 40, text: 'What is the deadline?' }),
      segment({ id: 's2', startTime: 80, text: 'Closing remarks' }),
    ];
    const markers = generateTimeline(segments);
    const question = markers.find((m) => m.kind === 'question');
    expect(question).toBeDefined();
    expect(question?.segmentIndex).toBe(1);
    expect(question?.segmentId).toBe('s1');
  });

  test('flags a resume marker after a long silence gap', () => {
    const segments = [
      segment({ id: 'a', startTime: 0, endTime: 5, text: 'Opening statements' }),
      segment({ id: 'b', startTime: 60, endTime: 63, text: 'Back after the break' }),
    ];
    const markers = generateTimeline(segments, { pauseThresholdSeconds: 20, minMarkerSpacingSeconds: 1 });
    expect(markers.some((m) => m.kind === 'resume' && m.segmentId === 'b')).toBe(true);
  });

  test('does not flag a resume marker for short gaps', () => {
    const segments = [
      segment({ id: 'a', startTime: 0, endTime: 5, text: 'First point' }),
      segment({ id: 'b', startTime: 10, endTime: 13, text: 'Second point' }),
    ];
    const markers = generateTimeline(segments, { pauseThresholdSeconds: 20 });
    expect(markers.some((m) => m.kind === 'resume')).toBe(false);
  });

  test('enforces minimum spacing by keeping the higher-priority marker', () => {
    const segments = [
      segment({ id: 'chapter', startTime: 0, text: 'General discussion' }),
      // An action item one second later must win over a coincident chapter.
      segment({ id: 'action', startTime: 1, text: "Let's ship it by tomorrow" }),
    ];
    const markers = generateTimeline(segments, { minMarkerSpacingSeconds: 8, targetChapters: 1 });
    expect(markers).toHaveLength(1);
    expect(markers[0].kind).toBe('action');
  });

  test('never exceeds the maxMarkers cap', () => {
    const segments = Array.from({ length: 100 }, (_, i) =>
      segment({ id: `s${i}`, startTime: i * 15, text: `What about option ${i}?` }),
    );
    const markers = generateTimeline(segments, { maxMarkers: 10, minMarkerSpacingSeconds: 1 });
    expect(markers.length).toBeLessThanOrEqual(10);
  });

  test('every marker points at a real segment', () => {
    const segments = [
      segment({ id: 'a', startTime: 0, text: 'Kickoff' }),
      segment({ id: 'b', startTime: 30, text: 'Who owns this task?' }),
      segment({ id: 'c', startTime: 90, text: "We'll review next week" }),
    ];
    const ids = new Set(segments.map((s) => s.id));
    const markers = generateTimeline(segments);
    expect(markers.length).toBeGreaterThan(0);
    for (const marker of markers) {
      expect(ids.has(marker.segmentId)).toBe(true);
      expect(marker.id.length).toBeGreaterThan(0);
    }
  });

  test('marker ids are unique within a generated timeline', () => {
    const segments = Array.from({ length: 40 }, (_, i) =>
      segment({ id: `s${i}`, startTime: i * 25, text: `Discussion ${i}` }),
    );
    const markers = generateTimeline(segments);
    const uniqueIds = new Set(markers.map((m) => m.id));
    expect(uniqueIds.size).toBe(markers.length);
  });
});
