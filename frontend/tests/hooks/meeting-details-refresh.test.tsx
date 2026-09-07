import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { useEffect } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SummaryProcessResponse } from '../../src/types';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalAnalytics = { ...await import('../../src/lib/analytics') };
const originalPreferences = { ...await import('../../src/lib/summary-language-preferences') };
const originalToast = { ...await import('sonner') };
const originalNavigation = { ...await import('next/navigation') };
const originalConfig = { ...await import('../../src/contexts/ConfigContext') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('../../src/lib/analytics', () => originalAnalytics);
  mock.module('../../src/lib/summary-language-preferences', () => originalPreferences);
  mock.module('sonner', () => originalToast);
  mock.module('next/navigation', () => originalNavigation);
  mock.module('../../src/contexts/ConfigContext', () => originalConfig);
});

let selectedMeeting = 'meeting-a';
mock.module('next/navigation', () => ({
  usePathname: () => '/meeting-details', useRouter: () => ({}),
  useSearchParams: () => new URLSearchParams({ id: selectedMeeting }),
}));
mock.module('../../src/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ isRecording: false }) }));
mock.module('../../src/contexts/ConfigContext', () => ({ useConfig: () => ({ isAutoSummary: false }) }));
const notify = mock(() => {});
mock.module('sonner', () => ({ toast: { info: notify, error: notify, success: notify, warning: notify } }));
mock.module('../../src/lib/analytics', () => ({ default: {
  trackPageView() {}, trackBackendConnection() {}, trackSummaryGenerationStarted: async () => {},
  trackSummaryGenerationCompleted: async () => {},
} }));
mock.module('../../src/lib/summary-language-preferences', () => ({
  readCachedDetectedSummaryLanguage: async () => null,
  detectAndCacheSummaryLanguage: async () => ({ language: 'en' }),
  readMeetingSummaryLanguage: async () => ({ language: 'en', storage: 'metadata' }),
}));
const metadata = (id: string) => ({ id, title: id, created_at: '2026-09-01', updated_at: '2026-09-01' });
let readMetadata: (id: string) => Promise<unknown>;
let readTranscripts: (id: string) => Promise<unknown>;
const transcriptPage = {
  transcripts: [{ id: 'transcript', text: 'Meeting transcript', timestamp: '00:00' }], total_count: 1, has_more: false,
};
let savedSummary: SummaryProcessResponse;
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meetings') return [];
  if (command === 'api_get_summary') return { ...savedSummary, meeting_id: args!.meetingId };
  if (command === 'api_get_meeting_metadata') return readMetadata(args!.meetingId as string);
  if (command === 'api_get_meeting_transcripts') return readTranscripts(args!.meetingId as string);
  if (command === 'api_process_transcript') return { process_id: 'attempt-b' };
  if (command === 'api_cancel_summary') return { cancelled: true };
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { SidebarProvider } = await import('../../src/components/Sidebar/SidebarProvider');
const { useSummaryGeneration } = await import('../../src/hooks/meeting-details/useSummaryGeneration');
const { useMeetingData } = await import('../../src/hooks/meeting-details/useMeetingData');

// Use a lightweight content view around the actual route, transcript loader, state hooks and polling provider.
let summaryState: ReturnType<typeof useSummaryGeneration>;
let refresh: () => Promise<void>;
let mounts = 0;
function SummaryView(props: any) {
  const data = useMeetingData(props);
  summaryState = useSummaryGeneration({
    meeting: props.meeting, initialSummary: props.initialSummary, transcripts: data.transcripts,
    modelConfig: { provider: 'ollama', model: 'test', whisperModel: 'base' }, isModelConfigLoading: false,
    selectedTemplate: 'daily_standup', setAiSummary: data.setAiSummary, updateMeetingTitle: data.updateMeetingTitle,
  });
  refresh = props.onRefetchTranscripts;
  useEffect(() => { mounts += 1; }, []);
  return <output>{props.meeting.id}:{summaryState.summaryStatus}:{JSON.stringify(data.aiSummary)}</output>;
}
mock.module('../../src/app/meeting-details/page-content', () => ({ default: SummaryView }));
const { default: MeetingDetails } = await import('../../src/app/meeting-details/page');

let renderer: ReactTestRenderer | undefined;
const timers = new Map<number, () => Promise<void>>();
let nextTimer = 0;
const realSetInterval = globalThis.setInterval;
const realClearInterval = globalThis.clearInterval;
beforeEach(() => {
  selectedMeeting = 'meeting-a';
  readMetadata = async id => metadata(id);
  readTranscripts = async () => transcriptPage;
  savedSummary = { meeting_id: selectedMeeting, status: 'pending', start: 'attempt-a', end: null, data: null, error: null, meetingName: 'Meeting A' };
  mounts = 0; timers.clear(); invoke.mockClear(); notify.mockClear();
  globalThis.setInterval = ((callback: () => Promise<void>) => {
    const id = ++nextTimer; timers.set(id, callback); return id;
  }) as typeof setInterval;
  globalThis.clearInterval = ((id: number) => { timers.delete(id); }) as typeof clearInterval;
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  globalThis.setInterval = realSetInterval;
  globalThis.clearInterval = realClearInterval;
});
async function show() {
  await act(async () => {
    const view = <SidebarProvider><MeetingDetails /></SidebarProvider>;
    if (renderer) renderer.update(view); else renderer = create(view);
  });
}
async function tick() {
  await act(async () => { for (const callback of [...timers.values()]) await callback(); });
}
const text = () => JSON.stringify(renderer!.toJSON());
async function complete(text: string, attempt: string) {
  savedSummary = { ...savedSummary, status: 'completed', start: attempt, data: { markdown: text } };
  await tick();
}
function deferMetadata() {
  let resolve!: (value: unknown) => void;
  readMetadata = () => new Promise(done => { resolve = done; });
  return async () => { await act(async () => { resolve(metadata(selectedMeeting)); }); };
}

describe('meeting route transcript refresh', () => {
  test('navigation waits for the new meeting first transcript page before mounting its view', async () => {
    await show();
    await complete('Summary A', 'attempt-a');
    selectedMeeting = 'meeting-b';
    savedSummary = { ...savedSummary, meeting_id: selectedMeeting, status: 'idle', data: null, start: null };
    let resolvePage!: (value: unknown) => void;
    readTranscripts = () => new Promise(resolve => { resolvePage = resolve; });
    await show();
    expect(renderer!.root.findAllByType('output')).toHaveLength(0);
    expect(mounts).toBe(1);
    await act(async () => { resolvePage(transcriptPage); });
    expect(text()).toContain('meeting-b');
    expect(summaryState.summaryStatus).toBe('idle');
    expect(text()).not.toContain('Summary A');
    expect(mounts).toBe(2);
  });

  test('completed regeneration survives refresh without reviving the original pending attempt', async () => {
    await show();
    await complete('Summary A', 'attempt-a');
    await act(async () => { await summaryState.handleRegenerateSummary(); });
    await complete('Summary B', 'attempt-b');
    expect(text()).toContain('Summary B');
    const finishRefresh = deferMetadata();
    await act(async () => { void refresh(); });
    expect(text()).toContain('Summary B');
    expect(timers.size).toBe(0);
    expect(mounts).toBe(1);
    await finishRefresh();
    await tick();
    expect(summaryState.summaryStatus).toBe('completed');
    expect(text()).toContain('Summary B');
    expect(timers.size).toBe(0);
    expect(mounts).toBe(1);
  });

  test('refresh during regeneration keeps the current attempt and its Stop token', async () => {
    await show();
    await complete('Summary A', 'attempt-a');
    await act(async () => { await summaryState.handleRegenerateSummary(); });
    const currentTimer = [...timers.keys()][0];
    const finishRefresh = deferMetadata();
    await act(async () => { void refresh(); });
    expect([...timers.keys()]).toEqual([currentTimer]);
    expect(summaryState.summaryStatus).toBe('regenerating');
    await finishRefresh();
    expect([...timers.keys()]).toEqual([currentTimer]);
    expect(summaryState.summaryStatus).toBe('regenerating');
    await act(async () => { await summaryState.handleStopGeneration(); });
    expect(invoke.mock.calls.find(([command]) => command === 'api_cancel_summary')?.[1]?.processId).toBe('attempt-b');
    expect(timers.size).toBe(0);
  });
});
