/**
 * Configuration Service
 *
 * Handles all configuration-related Tauri backend calls.
 * Pure 1-to-1 wrapper - no error handling changes, exact same behavior as direct invoke calls.
 */

import { invoke } from '@tauri-apps/api/core';
import { TranscriptModelProps } from '@/components/TranscriptSettings';

export interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openrouter' | 'openai';
  model: string;
  whisperModel: string;
}

export interface RecordingPreferences {
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
}

export interface LanguagePreference {
  language: string;
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
   * Get saved language preference
   * @returns Promise with language code string
   */
  async getLanguagePreference(): Promise<string> {
    return invoke<string>('get_language_preference');
  }
}

// Export singleton instance
export const configService = new ConfigService();
