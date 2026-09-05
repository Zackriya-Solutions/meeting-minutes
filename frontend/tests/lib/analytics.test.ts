import { describe, expect, mock, test } from 'bun:test';

const invokeMock = mock(async () => null);

mock.module('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));
// The analytics module must load after the Tauri invoke mock is registered.
const { Analytics } = await import('../../src/lib/analytics');

describe('summary generation analytics', () => {
  test('sends whole seconds and preserves an omitted duration', async () => {
    await Analytics.init();
    invokeMock.mockClear();

    await Analytics.trackSummaryGenerationCompleted('ollama', 'model', true, 1.999);
    await Analytics.trackSummaryGenerationCompleted('ollama', 'model', false, undefined, 'cancelled');

    expect(invokeMock.mock.calls).toEqual([
      [
        'track_summary_generation_completed',
        {
          modelProvider: 'ollama',
          modelName: 'model',
          success: true,
          durationSeconds: 1,
          errorMessage: undefined,
        },
      ],
      [
        'track_summary_generation_completed',
        {
          modelProvider: 'ollama',
          modelName: 'model',
          success: false,
          durationSeconds: undefined,
          errorMessage: 'cancelled',
        },
      ],
    ]);
  });
});
