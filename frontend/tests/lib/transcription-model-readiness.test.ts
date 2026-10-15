import { describe, expect, test } from 'bun:test';

import {
  getProviderCommands,
  hasDownloadingModel,
  isCloudProvider,
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

  test('does not silently treat an unsupported provider as Parakeet', () => {
    expect(getProviderCommands('deepgram')).toBeNull();
  });

  test('treats Deepgram as a cloud provider that needs no downloaded model', () => {
    expect(isCloudProvider('deepgram')).toBeTrue();
    expect(isCloudProvider('parakeet')).toBeFalse();
    expect(isCloudProvider('localWhisper')).toBeFalse();
    // Not accepted by the Rust start path, so it must not skip the local check.
    expect(isCloudProvider('openai')).toBeFalse();
  });

  test('recognizes only active downloads', () => {
    expect(hasDownloadingModel([{ status: 'Available' }])).toBeFalse();
    expect(hasDownloadingModel([{ status: 'Downloading' }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 0 } } }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 42 } } }])).toBeTrue();
  });
});
