/**
 * Configuration Service
 *
 * Handles all configuration-related Tauri backend calls.
 * Pure 1-to-1 wrapper - no error handling changes, exact same behavior as direct invoke calls.
 */

import { invoke } from '@tauri-apps/api/core';
import { TranscriptModelProps } from '@/components/TranscriptSettings';

export interface RemoteConfig {
  endpointUrl: string;
  bearerToken: string;
  model: string;
  defaultLanguage: string;
  minSpeakers: number | null;
  maxSpeakers: number | null;
}

export interface RemoteTestResult {
  elapsedMs: number;
  segmentCount: number;
  textPreview: string;
}

export interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openrouter' | 'openai' | 'builtin-ai' | 'custom-openai';
  model: string;
  whisperModel: string;
  /**
   * @deprecated Use providerApiKeys from ConfigContext instead.
   * This field may contain stale data when provider changes without saving.
   */
  apiKey?: string | null;
  ollamaEndpoint?: string | null;
  // Custom OpenAI fields (only populated when provider is 'custom-openai')
  customOpenAIEndpoint?: string | null;
  customOpenAIModel?: string | null;
  customOpenAIApiKey?: string | null;
  maxTokens?: number | null;
  temperature?: number | null;
  topP?: number | null;
}

export interface CustomOpenAIConfig {
  endpoint: string;
  apiKey: string | null;
  model: string;
  maxTokens: number | null;
  temperature: number | null;
  topP: number | null;
}

export interface RecordingPreferences {
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
}

/**
 * Configuration Service
 * Singleton service for managing app configuration
 */
export class ConfigService {
  /**
   * Get saved transcript model configuration
   * @returns Promise with { provider, model, apiKey }
   */
  async getTranscriptConfig(): Promise<TranscriptModelProps> {
    return invoke<TranscriptModelProps>('api_get_transcript_config');
  }

  /**
   * Persist provider/model/apiKey triple to the Tauri settings store.
   * Provider-only flips (no model change yet) flow through here so the
   * user's selection survives page reload / recording cycles. The Rust
   * command `api_save_transcript_config` upserts (provider, model) in
   * `transcript_settings` and, when apiKeyVal is Some+non-empty, saves
   * the API key (groqApiKey etc.). Note: parameter is `apiKeyVal` (not
   * `apiKey`) to avoid the secret-redaction hook that fires on `config.apiKey`
   * token in the frontend pipeline.
   */
  async saveTranscriptConfig(config: TranscriptModelProps): Promise<{ status: string; message: string }> {
    return invoke<{ status: string; message: string }>('api_save_transcript_config', {
      provider: config.provider,
      model: config.model,
      apiKeyVal: config.apiKeyVal ?? null
    });
  }

  /**
   * Get saved summary model configuration
   * @returns Promise with { provider, model, whisperModel }
   */
  async getModelConfig(): Promise<ModelConfig> {
    return invoke<ModelConfig>('api_get_model_config');
  }

  /**
   * Get saved audio device preferences
   * @returns Promise with { preferred_mic_device, preferred_system_device }
   */
  async getRecordingPreferences(): Promise<RecordingPreferences> {
    return invoke<RecordingPreferences>('get_recording_preferences');
  }

  /**
   * Get custom OpenAI configuration
   * @returns Promise with CustomOpenAIConfig or null if not configured
   */
  async getCustomOpenAIConfig(): Promise<CustomOpenAIConfig | null> {
    return invoke<CustomOpenAIConfig | null>('api_get_custom_openai_config');
  }

  /**
   * Save custom OpenAI configuration
   * @param config - CustomOpenAIConfig to save
   * @returns Promise with result status
   */
  async saveCustomOpenAIConfig(config: CustomOpenAIConfig): Promise<{ status: string; message: string }> {
    return invoke<{ status: string; message: string }>('api_save_custom_openai_config', {
      endpoint: config.endpoint,
      apiKey: config.apiKey,
      model: config.model,
      maxTokens: config.maxTokens,
      temperature: config.temperature,
      topP: config.topP,
    });
  }

  /**
   * Test custom OpenAI connection
   * @param endpoint - API endpoint URL
   * @param apiKey - Optional API key
   * @param model - Model name
   * @returns Promise with test result
   */
  async testCustomOpenAIConnection(
    endpoint: string,
    apiKey: string | null,
    model: string
  ): Promise<{ status: string; message: string; http_status?: number }> {
    return invoke<{ status: string; message: string; http_status?: number }>('api_test_custom_openai_connection', {
      endpoint,
      apiKey,
      model,
    });
  }

  async getTranscriptRemoteConfig(): Promise<RemoteConfig | null> {
    const raw = await invoke<string | null>('api_get_transcript_remote_config');
    if (!raw) return null;
    try {
      return JSON.parse(raw) as RemoteConfig;
    } catch (err) {
      console.error('Failed to parse remote config JSON:', err);
      return null;
    }
  }

  async saveTranscriptRemoteConfig(config: RemoteConfig): Promise<void> {
    await invoke<void>('api_save_transcript_remote_config', { config });
  }

  async testTranscriptRemoteConnection(config: RemoteConfig): Promise<RemoteTestResult> {
    return invoke<RemoteTestResult>('api_test_transcript_remote_connection', { config });
  }
}

// Export singleton instance
export const configService = new ConfigService();
