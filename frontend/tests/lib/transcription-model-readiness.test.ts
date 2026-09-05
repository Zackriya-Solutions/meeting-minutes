import { describe, expect, test } from 'bun:test';

import {
  getProviderCommands,
  hasDownloadingModel,
} from '../../src/lib/transcription-model-readiness';

describe('transcription model readiness', () => {
  test('uses Whisper commands when Whisper is the configured provider', () => {
    expect(getProviderCommands('localWhisper')).toEqual({
      initialize: 'whisper_init',
      hasAvailableModels: 'whisper_has_available_models',
      getAvailableModels: 'whisper_get_available_models',
    });
  });

  test('uses Parakeet commands when Parakeet is the configured provider', () => {
    expect(getProviderCommands('parakeet')).toEqual({
      initialize: 'parakeet_init',
      hasAvailableModels: 'parakeet_has_available_models',
      getAvailableModels: 'parakeet_get_available_models',
    });
  });

  test('uses Qwen commands when Qwen3 ASR is the configured provider', () => {
    expect(getProviderCommands('qwen3Asr')).toEqual({
      initialize: 'qwen_asr_init',
      hasAvailableModels: 'qwen_asr_has_available_models',
      getAvailableModels: 'qwen_asr_get_available_models',
    });
  });

  test('uses SenseVoice commands when SenseVoice is the configured provider', () => {
    expect(getProviderCommands('senseVoice')).toEqual({
      initialize: 'sense_voice_init',
      hasAvailableModels: 'sense_voice_has_available_models',
      getAvailableModels: 'sense_voice_get_available_models',
    });
  });

  test('does not silently treat an unsupported provider as Parakeet', () => {
    expect(getProviderCommands('deepgram')).toBeNull();
  });

  test('recognizes only active downloads', () => {
    expect(hasDownloadingModel([{ status: 'Available' }])).toBeFalse();
    expect(hasDownloadingModel([{ status: 'Downloading' }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 0 } } }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 42 } } }])).toBeTrue();
  });
});
