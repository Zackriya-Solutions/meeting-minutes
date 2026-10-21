import { describe, expect, mock, test } from 'bun:test';

let sessionSequence = 0;
const invokeMock = mock(async (command: string, _args?: unknown) =>
  command === 'start_analytics_session' ? `session_${++sessionSequence}` : null);

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

test('transcription error spike containment', async () => {
  Analytics.reset();
  try {
    await Analytics.init();
    await Analytics.startSession('test-user');
    invokeMock.mockClear();

    const cooldownStates = ['cooldown', 'cool down', 'cool-down', 'coolingdown', 'cooling down', 'cooling-down'];
    await Promise.all(Array.from({ length: 1000 }, (_, i) =>
      Analytics.trackTranscriptionError(
        `HTTP 401 ORT error: ${cooldownStates[i % cooldownStates.length].toUpperCase()} for ${i} seconds`,
      )));
    expect(invokeMock.mock.calls).toEqual([]);

    await Promise.all(Array.from({ length: 1000 }, (_, i) => [
      Analytics.trackTranscriptionError(`GAIA HTTP 401: unauthorized attempt ${i}`),
      Analytics.trackTranscriptionError(`Parakeet ORT error in chunk ${i}`),
      Analytics.trackTranscriptionError(`Unknown transcription failure for chunk ${i}`),
    ]).flat());

    const expectedEvents = ['auth_rejected', 'ort_failed', 'transcription_failed'].map<Parameters<typeof invokeMock>>(errorCode => [
      'track_event',
      {
        eventName: 'transcription_error',
        properties: {
          error_code: errorCode,
          timestamp: expect.any(String),
        },
      },
    ]);
    expect(invokeMock.mock.calls).toEqual(expectedEvents);

    await Analytics.startSession('test-user');
    invokeMock.mockClear();
    await Promise.all([
      Analytics.trackTranscriptionError('GAIA HTTP 401 again'),
      Analytics.trackTranscriptionError('GAIA HTTP 401 repeated in the new session'),
    ]);
    expect(invokeMock.mock.calls).toEqual([expectedEvents[0]]);
  } finally {
    Analytics.reset();
  }
});
