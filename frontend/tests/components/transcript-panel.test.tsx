import { afterAll, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { PlaybackSource, TranscriptSegmentData } from '../../src/types';
import { FakeAudio, audios } from '../fixtures/audio';

Object.defineProperty(globalThis, 'Audio', { configurable: true, value: FakeAudio });
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalView = { ...await import('../../src/components/VirtualizedTranscriptView') };
const originalButtons = { ...await import('../../src/components/MeetingDetails/TranscriptButtonGroup') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('../../src/components/VirtualizedTranscriptView', () => originalView);
  mock.module('../../src/components/MeetingDetails/TranscriptButtonGroup', () => originalButtons);
  Reflect.deleteProperty(globalThis, 'Audio');
});

let source: PlaybackSource = { path: '/recordings/meeting/audio.mp4', duration_s: 60, time_table: [[0, 0], [60, 60]] };
const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_prepare_meeting_playback') return source;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke, convertFileSrc: (path: string) => `asset://localhost/${path}` }));

// The transcript view and the button group stand in for the panel's children: their renders count.
type ViewProps = { activeSegmentId?: string | null; onPlayFrom?: (startS: number) => void };
type ButtonProps = { onRefetchTranscripts?: () => Promise<void> };
const renders = { view: 0, buttons: 0 };
let viewProps: ViewProps = {};
let buttonProps: ButtonProps = {};
mock.module('../../src/components/VirtualizedTranscriptView', () => ({
  ...originalView,
  VirtualizedTranscriptView: (props: ViewProps) => {
    renders.view += 1;
    viewProps = props;
    return null;
  },
}));
mock.module('../../src/components/MeetingDetails/TranscriptButtonGroup', () => ({
  ...originalButtons,
  TranscriptButtonGroup: (props: ButtonProps) => {
    renders.buttons += 1;
    buttonProps = props;
    return null;
  },
}));
const { TranscriptPanel } = await import('../../src/components/MeetingDetails/TranscriptPanel');

const segments: TranscriptSegmentData[] = [
  { id: 'a', timestamp: 0, endTime: 10, text: 'one', speaker: null },
  { id: 'b', timestamp: 10, endTime: 20, text: 'two', speaker: null },
  { id: 'c', timestamp: 20, endTime: 30, text: 'three', speaker: null },
];

describe('transcript panel during playback', () => {
  test('re-renders when the playing row changes, not on every timeupdate', async () => {
    const noop = () => {};
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(
        <TranscriptPanel
          transcripts={[]} customPrompt="" onPromptChange={noop} onCopyTranscript={noop} onOpenMeetingFolder={async () => {}}
          isRecording={false} usePagination segments={segments} hasMore={false} meetingId="meeting-a" meetingFolderPath="/recordings/meeting"
        />,
      );
    });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
    const audio = audios[audios.length - 1];
    await act(async () => { viewProps.onPlayFrom!(2); });
    expect(viewProps.activeSegmentId).toBe('a');

    const before = { ...renders };
    for (const time of [3, 4, 5, 6]) {
      audio.currentTime = time;
      await act(async () => { audio.emit('timeupdate'); });
    }
    expect(renders).toEqual(before);

    audio.currentTime = 12;
    await act(async () => { audio.emit('timeupdate'); });
    expect(viewProps.activeSegmentId).toBe('b');
    expect(renders.view).toBe(before.view + 1);
    await act(async () => renderer.unmount());
  });

  test('a reloaded transcript is placed in the audio by a reloaded time table', async () => {
    const noop = () => {};
    const refetch = mock(async () => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(
        <TranscriptPanel
          transcripts={[]} customPrompt="" onPromptChange={noop} onCopyTranscript={noop} onOpenMeetingFolder={async () => {}}
          isRecording={false} usePagination segments={segments} hasMore={false} meetingId="meeting-a" meetingFolderPath="/recordings/meeting"
          onRefetchTranscripts={refetch}
        />,
      );
    });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
    const audio = audios[audios.length - 1];
    await act(async () => { viewProps.onPlayFrom!(45); });
    expect(audio.currentTime).toBe(45);

    // A retranscription times the new rows by the decoded file, which this table maps to the audio.
    source = { ...source, time_table: [[0, 0], [0.02, 0], [30.02, 30], [30.06, 30], [60.06, 60]] };
    await act(async () => { await buttonProps.onRefetchTranscripts!(); });
    expect(refetch).toHaveBeenCalledTimes(1);
    await act(async () => { viewProps.onPlayFrom!(45.06); });
    expect(Math.abs(audio.currentTime - 45)).toBeLessThan(1e-9);
    await act(async () => renderer.unmount());
  });
});
