import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { renderToStaticMarkup } from 'react-dom/server';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Bun shares module mocks between test files; restore the real modules after this suite.
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
});

const toastError = mock((_message: string, _options?: unknown) => {});
mock.module('sonner', () => ({ toast: { error: toastError, info() {}, success() {}, warning() {} } }));

let load: () => Promise<{ context: string }>;
const saved: unknown[] = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_global_summary_context') return load();
  if (command === 'api_save_global_summary_context') {
    saved.push(args);
    return undefined;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { SummaryContextSettings } = await import('../../src/components/SummaryContextSettings');

const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });

async function mount(): Promise<ReactTestRenderer> {
  let renderer!: ReactTestRenderer;
  await act(async () => { renderer = create(<SummaryContextSettings />); });
  await settle();
  return renderer;
}

beforeEach(() => {
  saved.length = 0;
  toastError.mockClear();
  load = async () => ({ context: '' });
});

describe('global summary context settings', () => {
  test('explains what the text is for and starts disabled until it has loaded', () => {
    const html = renderToStaticMarkup(<SummaryContextSettings />);
    expect(html).toContain('Summary Context');
    expect(html).toContain(
      'Sent with every summary. Use it for things that are always true: who you are, your organization, names, and terms.',
    );
    expect(html.toLowerCase()).toContain('maxlength="4000"');
    expect(html).toContain('disabled=""');
  });

  test('a loaded value is shown and editable', async () => {
    load = async () => ({ context: 'I am Noah.' });
    const renderer = await mount();
    const area = renderer.root.findByType('textarea');
    expect(area.props.value).toBe('I am Noah.');
    expect(area.props.disabled).toBe(false);
    await act(async () => renderer.unmount());
  });

  test('a failed load leaves the box disabled and saves nothing', async () => {
    load = async () => { throw new Error('db locked'); };
    const renderer = await mount();
    expect(toastError.mock.calls[0]?.[0]).toBe('Failed to load summary context');
    expect(renderer.root.findByType('textarea').props.disabled).toBe(true);
    await act(async () => renderer.unmount());
    await settle();
    expect(saved).toEqual([]);
  });
});
