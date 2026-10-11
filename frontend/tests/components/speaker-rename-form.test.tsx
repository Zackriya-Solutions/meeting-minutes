import { afterAll, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { Person } from '../../src/types';

const originalToast = { ...await import('sonner') };
afterAll(() => mock.module('sonner', () => originalToast));
mock.module('sonner', () => ({ toast: { error: () => {}, success: () => {}, info: () => {}, warning: () => {} } }));
const { SpeakerRenameForm } = await import('../../src/components/Speakers/SpeakerRenameForm');
const { PeopleContext } = await import('../../src/components/Speakers/PeopleContext');

const people: Person[] = [
  { id: 'person-1', name: 'Noah', meeting_count: 3, last_seen: null },
  { id: 'person-2', name: 'Ana', meeting_count: 1, last_seen: null },
];

async function renderForm(onRename: (key: string, name: string) => Promise<void>, onSaved = () => {}) {
  let renderer!: ReactTestRenderer;
  await act(async () => {
    renderer = create(
      <PeopleContext.Provider value={people}>
        <SpeakerRenameForm speakerKey="spk_0" initialName="" placeholder="Speaker 1" onRename={onRename} onSaved={onSaved} />
      </PeopleContext.Provider>,
    );
  });
  return renderer;
}
async function type(renderer: ReactTestRenderer, value: string) {
  const input = renderer.root.find((n) => n.type === 'input');
  await act(async () => { input.props.onChange({ target: { value } }); });
}

describe('speaker name form', () => {
  test('picking a person names with their exact name', async () => {
    const onRename = mock(async (_key: string, _name: string) => {});
    const onSaved = mock(() => {});
    const renderer = await renderForm(onRename, onSaved);
    await type(renderer, 'noa');
    expect(renderer.root.findAll((n) => n.type === 'button' && n.props['aria-label'] === 'Name as Ana')).toHaveLength(0);
    const option = renderer.root.find((n) => n.type === 'button' && n.props['aria-label'] === 'Name as Noah');
    await act(async () => { await option.props.onClick(); });
    expect(onRename).toHaveBeenCalledWith('spk_0', 'Noah');
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  test('typing a new name submits the text', async () => {
    const onRename = mock(async (_key: string, _name: string) => {});
    const renderer = await renderForm(onRename);
    await type(renderer, 'Zoe');
    expect(renderer.root.findAll((n) => n.type === 'button' && String(n.props['aria-label']).startsWith('Name as'))).toHaveLength(0);
    const form = renderer.root.find((n) => n.type === 'form');
    await act(async () => { await form.props.onSubmit({ preventDefault: () => {} }); });
    expect(onRename).toHaveBeenCalledWith('spk_0', 'Zoe');
  });
});
