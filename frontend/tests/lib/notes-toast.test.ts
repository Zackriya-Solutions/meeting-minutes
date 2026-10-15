import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});

type ToastOptions = {
  description: string;
  duration: number;
  action: { label: string; onClick: (event: { preventDefault: () => void }) => void };
};
const toastError = mock((_title: string, _options?: unknown) => {});
const toastWarning = mock((_title: string, _options?: unknown) => {});
const toastSuccess = mock((_title: string) => {});
mock.module('sonner', () => ({ toast: { error: toastError, warning: toastWarning, success: toastSuccess } }));
const { notifyUnsavedNotes } = await import('../../src/lib/notes-toast');

const clipboardDescriptor = Object.getOwnPropertyDescriptor(globalThis.navigator, 'clipboard');
let writeText: ReturnType<typeof mock>;

beforeEach(() => {
  toastError.mockClear();
  toastWarning.mockClear();
  toastSuccess.mockClear();
  writeText = mock(async (_text: string) => {});
  Object.defineProperty(globalThis.navigator, 'clipboard', { value: { writeText }, configurable: true });
});

afterEach(() => {
  if (clipboardDescriptor) Object.defineProperty(globalThis.navigator, 'clipboard', clipboardDescriptor);
  else delete (globalThis.navigator as { clipboard?: unknown }).clipboard;
});

const options = { title: "Couldn't save notes", description: 'Copy them now.', markdown: 'Ship Friday' };
const click = async (toastMock: typeof toastError) => {
  const shown = toastMock.mock.calls[0][1] as ToastOptions;
  const event = { preventDefault: mock(() => {}) };
  shown.action.onClick(event);
  await new Promise((resolve) => setTimeout(resolve, 0));
  return event;
};

describe('notifyUnsavedNotes', () => {
  test('shows a toast at the requested level that stays until dismissed', () => {
    notifyUnsavedNotes({ level: 'error', ...options });
    notifyUnsavedNotes({ level: 'warning', ...options });

    expect(toastError.mock.calls[0][0]).toBe("Couldn't save notes");
    expect(toastError.mock.calls[0][1]).toMatchObject({
      description: 'Copy them now.',
      duration: Infinity,
      action: { label: 'Copy notes' },
    });
    expect(toastWarning.mock.calls[0][0]).toBe("Couldn't save notes");
    expect(toastWarning.mock.calls).toHaveLength(1);
    expect(toastError.mock.calls).toHaveLength(1);
  });

  test('copying keeps the toast open, copies the text and confirms', async () => {
    notifyUnsavedNotes({ level: 'error', ...options });

    const event = await click(toastError);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(writeText).toHaveBeenCalledWith('Ship Friday');
    expect(toastSuccess).toHaveBeenCalledWith('Notes copied to clipboard');
  });

  test('says so when the copy fails', async () => {
    writeText = mock(async () => { throw new Error('denied'); });
    Object.defineProperty(globalThis.navigator, 'clipboard', { value: { writeText }, configurable: true });
    notifyUnsavedNotes({ level: 'warning', ...options });

    await click(toastWarning);

    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastError).toHaveBeenCalledWith('Failed to copy notes');
  });
});
