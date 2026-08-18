import { describe, expect, test } from 'bun:test';
import { startAudioImportCommand } from '../../src/services/importAudioService';

describe('startAudioImportCommand', () => {
  test('threads deliberate re-import and denoise intent through the Tauri boundary', async () => {
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const invoke = async (command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      return undefined;
    };

    await startAudioImportCommand({
      sourcePath: '/tmp/call.wav',
      title: 'Call',
      language: null,
      model: 'gigaam-v3-e2e-ctc',
      provider: 'gigaam',
      denoiseAudio: true,
      forceReimport: true,
    }, invoke);

    expect(calls).toEqual([{
      command: 'start_import_audio_command',
      args: {
        sourcePath: '/tmp/call.wav',
        title: 'Call',
        language: null,
        model: 'gigaam-v3-e2e-ctc',
        provider: 'gigaam',
        denoiseAudio: true,
        forceReimport: true,
      },
    }]);
    expect(calls[0].args).not.toHaveProperty('force_reimport');
    expect(calls[0].args).not.toHaveProperty('denoise_audio');
  });
});
