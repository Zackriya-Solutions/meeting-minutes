export interface ModelWithStatus {
  status?: unknown;
}

interface ProviderCommands {
  initialize: string;
  hasAvailableModels: string;
  getAvailableModels: string;
}

const PROVIDER_COMMANDS: Record<string, ProviderCommands> = {
  localWhisper: {
    initialize: 'whisper_init',
    hasAvailableModels: 'whisper_has_available_models',
    getAvailableModels: 'whisper_get_available_models',
  },
  parakeet: {
    initialize: 'parakeet_init',
    hasAvailableModels: 'parakeet_has_available_models',
    getAvailableModels: 'parakeet_get_available_models',
  },
  qwen3Asr: {
    initialize: 'qwen_asr_init',
    hasAvailableModels: 'qwen_asr_has_available_models',
    getAvailableModels: 'qwen_asr_get_available_models',
  },
  senseVoice: {
    initialize: 'sense_voice_init',
    hasAvailableModels: 'sense_voice_has_available_models',
    getAvailableModels: 'sense_voice_get_available_models',
  },
};

export function getProviderCommands(provider: string): ProviderCommands | null {
  return PROVIDER_COMMANDS[provider] ?? null;
}

export function hasDownloadingModel(models: ModelWithStatus[]): boolean {
  return models.some(({ status }) => (
    status === 'Downloading'
    || (status !== null && typeof status === 'object' && 'Downloading' in status)
  ));
}
