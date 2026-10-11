import { afterAll, describe, expect, mock, spyOn, test } from 'bun:test';

const originalToast = { ...await import('sonner') };
afterAll(() => mock.module('sonner', () => originalToast));
const failure = mock((_message: string) => {});
mock.module('sonner', () => ({ toast: { error: failure, success: () => {}, info: () => {}, warning: () => {} } }));
const { attempt, errorMessage } = await import('../../src/lib/errors');

describe('errorMessage', () => {
  test('backend refusals arrive as strings and are shown as written', () => {
    expect(errorMessage('A person named Noah already exists', 'Failed to rename')).toBe('A person named Noah already exists');
  });

  test('an Error shows its message; anything else shows the fallback', () => {
    expect(errorMessage(new Error('Network down'), 'Failed to rename')).toBe('Network down');
    expect(errorMessage({ code: 1 }, 'Failed to rename')).toBe('Failed to rename');
    expect(errorMessage(undefined, 'Failed to rename')).toBe('Failed to rename');
  });
});

describe('attempt', () => {
  test('reports success and shows nothing', async () => {
    failure.mockClear();
    expect(await attempt(async () => {}, 'Failed to confirm the name')).toBe(true);
    expect(failure).not.toHaveBeenCalled();
  });

  test('a failure is logged and shown with the backend text', async () => {
    failure.mockClear();
    const logged = spyOn(console, 'error').mockImplementation(() => {});
    expect(await attempt(async () => { throw 'A speaker job is running'; }, 'Failed to confirm the name')).toBe(false);
    expect(failure).toHaveBeenCalledWith('A speaker job is running');
    expect(await attempt(async () => { throw { code: 1 }; }, 'Failed to confirm the name')).toBe(false);
    expect(failure).toHaveBeenLastCalledWith('Failed to confirm the name');
    expect(logged).toHaveBeenCalledTimes(2);
    logged.mockRestore();
  });
});
