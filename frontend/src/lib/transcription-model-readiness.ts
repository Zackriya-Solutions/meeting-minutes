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
};

/**
 * Cloud providers stream audio to an API and need no downloaded model. The
 * Rust start command still validates them (e.g. that a Deepgram key is
 * configured) before capture begins.
 * DO NOT add a provider here unless start_recording accepts it.
 */
const CLOUD_PROVIDERS = new Set(['deepgram']);

export function isCloudProvider(provider: string): boolean {
  return CLOUD_PROVIDERS.has(provider);
}

export function getProviderCommands(provider: string): ProviderCommands | null {
  return PROVIDER_COMMANDS[provider] ?? null;
}

export function hasDownloadingModel(models: ModelWithStatus[]): boolean {
  return models.some(({ status }) => (
    status === 'Downloading'
    || (status !== null && typeof status === 'object' && 'Downloading' in status)
  ));
}
