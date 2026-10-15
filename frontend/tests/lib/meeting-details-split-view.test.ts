import { describe, expect, test } from 'bun:test';
import { resolveRightPane } from '../../src/components/MeetingDetails/MeetingDetailsSplitView';

describe('resolveRightPane', () => {
  test('shows the selected pane when Summary or Notes is the active tab', () => {
    expect(resolveRightPane('summary', 'notes')).toBe('summary');
    expect(resolveRightPane('notes', 'summary')).toBe('notes');
  });

  test('keeps the last right-hand pane while the transcript tab is active', () => {
    expect(resolveRightPane('transcript', 'notes')).toBe('notes');
    expect(resolveRightPane('transcript', 'summary')).toBe('summary');
  });
});
