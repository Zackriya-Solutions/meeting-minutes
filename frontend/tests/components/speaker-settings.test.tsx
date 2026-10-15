import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import type { ReactNode } from 'react';
import { act, create, type ReactTestInstance, type ReactTestRenderer } from 'react-test-renderer';
import type { RecordingPreferences } from '../../src/components/RecordingSettings';
import type { Person } from '../../src/types';
import { formatLastSeen } from '../../src/lib/people';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
const originalDialog = { ...await import('../../src/components/ui/dialog') };
const originalSwitch = { ...await import('../../src/components/ui/switch') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
  mock.module('../../src/components/ui/dialog', () => originalDialog);
  mock.module('../../src/components/ui/switch', () => originalSwitch);
});

// Dialogs render inline while open; the switch is a plain button.
const passthrough = ({ children }: { children?: ReactNode }) => <>{children}</>;
mock.module('../../src/components/ui/dialog', () => ({
  ...originalDialog,
  Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) => (open ? <>{children}</> : null),
  DialogContent: passthrough, DialogHeader: passthrough, DialogFooter: passthrough, DialogTitle: passthrough, DialogDescription: passthrough,
}));
mock.module('../../src/components/ui/switch', () => ({
  Switch: (props: { checked?: boolean; onCheckedChange?: (checked: boolean) => void; 'aria-label'?: string }) => (
    <button type="button" role="switch" aria-label={props['aria-label']} aria-checked={props.checked}
      onClick={() => props.onCheckedChange?.(!props.checked)} />
  ),
}));
mock.module('sonner', () => ({ toast: { error: () => {}, success: () => {}, info: () => {}, warning: () => {} } }));

const preferences: RecordingPreferences = {
  save_folder: '/recordings', auto_save: true, file_format: 'mp4', preferred_mic_device: 'USB Mic',
  preferred_system_device: null, identify_speakers_after_recording: false, remember_voices: true,
};
const noah: Person = { id: 'person-1', name: 'Noah', meeting_count: 3, last_seen: '2026-10-01T10:00:00+00:00' };
const ana: Person = { id: 'person-2', name: 'Ana', meeting_count: 1, last_seen: null };
let renameError: string | null = null;
const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'get_recording_preferences') return preferences;
  if (command === 'api_list_people') return [ana, noah];
  if (command === 'api_rename_person') {
    if (renameError) throw renameError;
    return null;
  }
  if (['set_recording_preferences', 'api_merge_people', 'api_forget_person', 'api_forget_all_voices'].includes(command)) return null;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { SpeakerSettings } = await import('../../src/components/Speakers/SpeakerSettings');

let renderer: ReactTestRenderer | undefined;
async function settle() {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}
async function mount() {
  await act(async () => { renderer = create(<SpeakerSettings />); });
  await settle();
}
/** Text under a test instance (the shared `textOf` reads JSON trees). */
function instanceText(node: ReactTestInstance | string): string {
  return typeof node === 'string' ? node : node.children.map(instanceText).join('');
}
const allText = () => instanceText(renderer!.root);
const buttons = () => renderer!.root.findAll((n) => n.type === 'button');
const byLabel = (label: string) => buttons().find((n) => n.props['aria-label'] === label)!;
const byText = (text: string) => buttons().find((n) => instanceText(n) === text)!;
async function press(node: ReactTestInstance) {
  expect(node).toBeDefined();
  await act(async () => { await node.props.onClick(); });
  await settle();
}
const calls = (command: string) => invoke.mock.calls.filter(([name]) => name === command);

beforeEach(() => {
  renameError = null;
  invoke.mockClear();
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

describe('Settings → Speakers', () => {
  test('turning remember voices off saves and offers forget all', async () => {
    await mount();
    expect(allText()).not.toContain('Forget all voices?');
    await press(byLabel('Remember voices across meetings'));
    expect(calls('set_recording_preferences')).toHaveLength(1);
    expect(allText()).toContain('Forget all voices?');
    await press(byText('Forget all'));
    expect(calls('api_forget_all_voices')).toHaveLength(1);
  });

  test('saving keeps other recording preferences', async () => {
    await mount();
    await press(byLabel('Remember voices across meetings'));
    expect(calls('set_recording_preferences')[0][1]).toEqual({ preferences: { ...preferences, remember_voices: false } });
  });

  test('forget all requires confirmation', async () => {
    await mount();
    await press(byText('Forget all voices…'));
    expect(calls('api_forget_all_voices')).toHaveLength(0);
    expect(allText()).toContain('Forget all voices?');
    const listed = calls('api_list_people').length;
    await press(byText('Forget all'));
    expect(calls('api_forget_all_voices')).toHaveLength(1);
    expect(calls('api_list_people')).toHaveLength(listed + 1);
  });

  test('rename person calls command and refreshes', async () => {
    await mount();
    await press(byLabel('Rename Noah'));
    const input = renderer!.root.find((n) => n.type === 'input' && n.props['aria-label'] === 'New name for Noah');
    await act(async () => { input.props.onChange({ target: { value: 'Noah P' } }); });
    const listed = calls('api_list_people').length;
    const form = renderer!.root.find((n) => n.type === 'form');
    await act(async () => { await form.props.onSubmit({ preventDefault: () => {} }); });
    await settle();
    expect(invoke).toHaveBeenCalledWith('api_rename_person', { personId: 'person-1', name: 'Noah P' });
    expect(calls('api_list_people')).toHaveLength(listed + 1);
  });

  test('rename collision shows the message', async () => {
    renameError = 'A person named Ana already exists';
    await mount();
    await press(byLabel('Rename Noah'));
    const input = renderer!.root.find((n) => n.type === 'input' && n.props['aria-label'] === 'New name for Noah');
    await act(async () => { input.props.onChange({ target: { value: 'ana' } }); });
    const form = renderer!.root.find((n) => n.type === 'form');
    await act(async () => { await form.props.onSubmit({ preventDefault: () => {} }); });
    await settle();
    const alert = renderer!.root.find((n) => n.type === 'p' && n.props.role === 'alert');
    expect(instanceText(alert)).toBe('A person named Ana already exists');
  });

  test('merge moves into the selected person', async () => {
    await mount();
    await press(byLabel('Merge Noah'));
    await press(byLabel('Merge Noah into Ana'));
    expect(invoke).toHaveBeenCalledWith('api_merge_people', { fromId: 'person-1', intoId: 'person-2' });
  });

  test('people list shows meeting count and last seen', async () => {
    await mount();
    expect(allText()).toContain(`3 meetings · last seen ${formatLastSeen(noah.last_seen)}`);
    expect(allText()).toContain('1 meeting · last seen never');
  });
});
