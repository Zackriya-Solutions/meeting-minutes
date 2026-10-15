import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestInstance, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
});

const toastSuccess = mock((_message: string) => {});
const toastError = mock((_message: string) => {});
mock.module('sonner', () => ({ toast: { error: toastError, info() {}, success: toastSuccess, warning() {} } }));

const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const standard = {
  id: 'standard_meeting',
  kind: 'builtin' as const,
  name: 'Standard Meeting Notes',
  description: 'General meetings',
  sections: [
    {
      title: 'Summary',
      instruction: 'Provide a brief summary.',
      format: 'paragraph',
    },
  ],
};

let templates = [
  { id: standard.id, name: standard.name, description: standard.description, kind: standard.kind },
];
let hiddenCount = 0;

const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  calls.push({ command, args });
  if (command === 'api_list_templates') return templates;
  if (command === 'api_hidden_template_count') return hiddenCount;
  if (command === 'api_get_template') return standard;
  if (command === 'api_save_template') {
    const template = args?.template as { name: string; description: string; sections: typeof standard.sections };
    return {
      ...standard,
      id: (args?.id as string | null) ?? 'client_call',
      kind: args?.id ? 'edited' : 'custom',
      name: template.name,
      description: template.description,
      sections: template.sections,
    };
  }
  if (command === 'api_delete_template') {
    templates = [];
    hiddenCount = 1;
    return undefined;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));

const { SummaryTemplateSettings } = await import('../../src/components/SummaryTemplateSettings');

const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });

function textOf(node: ReactTestInstance | string): string {
  if (typeof node === 'string') return node;
  return node.children.map(child => textOf(child as ReactTestInstance | string)).join('');
}

function button(renderer: ReactTestRenderer, label: string) {
  const match = renderer.root.findAllByType('button').find(node => textOf(node).includes(label));
  if (!match) throw new Error(`No button containing "${label}"`);
  return match;
}

function field(renderer: ReactTestRenderer, label: string) {
  const match = renderer.root.findAll(node => node.props['aria-label'] === label)[0];
  if (!match) throw new Error(`No field labeled "${label}"`);
  return match;
}

async function mount(): Promise<ReactTestRenderer> {
  let renderer!: ReactTestRenderer;
  await act(async () => { renderer = create(<SummaryTemplateSettings />); });
  await settle();
  return renderer;
}

beforeEach(() => {
  calls.length = 0;
  toastSuccess.mockClear();
  toastError.mockClear();
  templates = [
    { id: standard.id, name: standard.name, description: standard.description, kind: standard.kind },
  ];
  hiddenCount = 0;
});

describe('summary template settings', () => {
  test('shows the first template and keeps save disabled until it changes', async () => {
    const renderer = await mount();
    expect(textOf(renderer.root)).toContain('Summary templates');
    expect(textOf(renderer.root)).toContain('Standard Meeting Notes');
    expect(field(renderer, 'Template description').props.value).toBe('General meetings');
    expect(field(renderer, 'Section 1 instruction').props.value).toBe('Provide a brief summary.');
    expect(button(renderer, 'Save').props.disabled).toBe(true);
    await act(async () => renderer.unmount());
  });

  test('saves edits to the current template id', async () => {
    const renderer = await mount();
    await act(async () => {
      field(renderer, 'Template name').props.onChange({ target: { value: 'Weekly notes' } });
    });
    await act(async () => { button(renderer, 'Save').props.onClick(); });
    await settle();

    const save = calls.find(call => call.command === 'api_save_template');
    expect(save?.args).toMatchObject({
      id: 'standard_meeting',
      template: { name: 'Weekly notes', description: 'General meetings' },
    });
    expect(toastSuccess.mock.calls[0]?.[0]).toBe('Saved Weekly notes');
    await act(async () => renderer.unmount());
  });

  test('creates a template with a new id', async () => {
    const renderer = await mount();
    await act(async () => { button(renderer, 'New template').props.onClick(); });
    await act(async () => {
      field(renderer, 'Template name').props.onChange({ target: { value: 'Client call' } });
      field(renderer, 'Template description').props.onChange({ target: { value: 'A customer call' } });
      field(renderer, 'Section 1 title').props.onChange({ target: { value: 'Outcome' } });
      field(renderer, 'Section 1 instruction').props.onChange({ target: { value: 'Record the outcome' } });
    });
    await act(async () => { button(renderer, 'Save').props.onClick(); });
    await settle();

    const save = calls.find(call => call.command === 'api_save_template');
    expect(save?.args?.id).toBeNull();
    expect(save?.args?.template).toMatchObject({ name: 'Client call' });
    await act(async () => renderer.unmount());
  });

  test('asks before deleting and then hides a built-in template', async () => {
    const renderer = await mount();
    await act(async () => { button(renderer, 'Delete').props.onClick(); });
    expect(textOf(renderer.root)).toContain('Hide "Standard Meeting Notes"?');
    expect(calls.some(call => call.command === 'api_delete_template')).toBe(false);

    await act(async () => { button(renderer, 'Delete').props.onClick(); });
    await settle();

    expect(calls.some(call => call.command === 'api_delete_template' && call.args?.templateId === 'standard_meeting')).toBe(true);
    expect(toastSuccess.mock.calls[0]?.[0]).toBe('Hid Standard Meeting Notes');
    expect(textOf(renderer.root)).toContain('No templates.');
    await act(async () => renderer.unmount());
  });
});
