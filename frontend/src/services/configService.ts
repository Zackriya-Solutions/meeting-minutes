/**
 * Configuration Service
 *
 * Handles all configuration-related Tauri backend calls.
 * Pure 1-to-1 wrapper - no error handling changes, exact same behavior as direct invoke calls.
 */

import { invoke } from '@tauri-apps/api/core';
import type { TranscriptModelProps } from '@/components/TranscriptSettings';

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
 * Configuration for a self-hosted realtime (websocket) transcription endpoint,
 * e.g. a vLLM server hosting Voxtral Realtime. Mirrors the backend
 * `CustomTranscriptionConfig` (JSON in `transcript_settings.customTranscriptionConfig`).
 */
export interface CustomTranscriptionConfig {
  /** Base URL of the endpoint (ws://, wss://, http:// or https://). */
  endpoint: string;
  /** Optional bearer token (null/empty if the server needs none). */
  apiKey: string | null;
  /** Model identifier requested from the server. */
  model: string;
  /** Streaming protocol dialect. Defaults to "voxtral-realtime". */
  protocol: string;
  /** Optional requested transcription delay (ms); protocol-specific, may be ignored. */
  delayMs: number | null;
  /**
   * Seconds of audio per server session before the client rolls over to a fresh
   * one. `null` uses the backend default (300s), `0` disables rollover.
   */
  maxSessionSeconds: number | null;
}

/**
 * What a realtime endpoint reports about how much audio one session can hold.
 * Mirrors the backend `DetectedSessionLimit` (serialized snake_case).
 */
export interface DetectedSessionLimit {
  /** Context window in tokens, or null when the endpoint announces none. */
  max_model_len: number | null;
  /** Session length derived from that context, in seconds. */
  recommended_seconds: number | null;
  /** Plain-language account of what was found. */
  detail: string;
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

  /**
   * Get the custom streaming (websocket) transcription configuration.
   * @returns Promise with CustomTranscriptionConfig or null if not configured
   */
  async getCustomTranscriptionConfig(): Promise<CustomTranscriptionConfig | null> {
    return invoke<CustomTranscriptionConfig | null>('api_get_custom_transcription_config');
  }

  /**
   * Save the custom streaming transcription configuration. This also activates
   * the streaming provider (sets the transcript provider to "customStreaming").
   * @param config - CustomTranscriptionConfig to save
   * @returns Promise with result status
   */
  async saveCustomTranscriptionConfig(
    config: CustomTranscriptionConfig
  ): Promise<{ status: string; message: string }> {
    return invoke<{ status: string; message: string }>('api_save_custom_transcription_config', {
      endpoint: config.endpoint,
      model: config.model,
      apiKey: config.apiKey,
      protocol: config.protocol,
      delayMs: config.delayMs,
      maxSessionSeconds: config.maxSessionSeconds,
    });
  }

  /**
   * Ask the endpoint how much audio it can hold in one session (OpenAI-style
   * `/v1/models` context size). Resolves with `recommended_seconds: null` when the
   * server announces nothing — that is an answer, not a failure.
   */
  async detectCustomTranscriptionLimits(
    endpoint: string,
    model: string,
    apiKey: string | null,
    protocol: string = 'voxtral-realtime'
  ): Promise<DetectedSessionLimit> {
    return invoke<DetectedSessionLimit>('api_detect_custom_transcription_limits', {
      endpoint,
      model,
      apiKey,
      protocol,
    });
  }

  /**
   * Test connectivity to a realtime transcription endpoint (connects, performs
   * the protocol handshake — which validates the model — and disconnects).
   * @returns Promise with test result; rejects with an error message on failure
   */
  async testCustomTranscriptionConnection(
    endpoint: string,
    model: string,
    apiKey: string | null,
    protocol: string = 'voxtral-realtime',
    delayMs: number | null = null
  ): Promise<{ status: string; message: string }> {
    return invoke<{ status: string; message: string }>('api_test_custom_transcription_connection', {
      endpoint,
      model,
      apiKey,
      protocol,
      delayMs,
    });
  }
}

// Export singleton instance
export const configService = new ConfigService();
