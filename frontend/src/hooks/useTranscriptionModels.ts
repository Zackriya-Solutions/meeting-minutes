import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '@/lib/i18n';

export interface RawModelInfo {
  name: string;
  size_mb: number;
  status: 'Available' | 'Missing' | { Downloading: { progress: number } } | { Error: string };
}

export interface ModelOption {
  provider: 'whisper' | 'parakeet' | 'gigaam' | 'salutespeech';
  name: string;
  displayName: string;
  size_mb: number;
}

// Must match SALUTE_MODEL in TranscriptSettings.tsx (the model string persisted for the
// SaluteSpeech provider). The actual recognition model is chosen in Settings.
const SALUTE_MODEL_NAME = 'salutespeech-stream-v2';

interface GigaamStatus {
  model: string;
  model_present: boolean;
  loaded: boolean;
}

interface TranscriptModelConfig {
  provider?: string;
  model?: string;
}

/**
 * Custom hook for fetching and managing transcription models (Whisper and Parakeet).
 *
 * This hook centralizes the model fetching logic that was previously duplicated
 * in ImportAudioDialog and RetranscribeDialog components.
 *
 * @param transcriptModelConfig - User's saved model configuration from context
 * @returns Object containing available models, selected model key, loading state, and fetch function
 */
export function useTranscriptionModels(transcriptModelConfig: TranscriptModelConfig | undefined) {
  const t = useT();
  const [availableModels, setAvailableModels] = useState<ModelOption[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState<string>('');
  const [loadingModels, setLoadingModels] = useState(false);
  // Track whether the user has manually changed the model selection
  const userSelectedRef = useRef(false);

  // Wrap setSelectedModelKey to track user-initiated changes
  const setSelectedModelKeyWithTracking = useCallback((key: string) => {
    userSelectedRef.current = true;
    setSelectedModelKey(key);
  }, []);

  const fetchModels = useCallback(async () => {
    setLoadingModels(true);
    const allModels: ModelOption[] = [];

    // Fetch GigaAM (the primary/default local engine) first so it's the default choice.
    try {
      const gigaam = await invoke<GigaamStatus>('gigaam_status');
      if (gigaam?.model_present) {
        allModels.push({
          provider: 'gigaam' as const,
          name: gigaam.model || 'gigaam-v3-e2e-ctc',
          displayName: t('GigaAM v3 (Russian)'),
          size_mb: 224,
        });
      }
    } catch (err) {
      console.error('Failed to fetch GigaAM status:', err);
    }

    // SaluteSpeech (cloud) — only offered when a Sber Authorization Key is configured.
    try {
      const settings = await invoke<Record<string, string>>('get_app_settings');
      if (settings?.['salutespeech.auth_key'] && settings['salutespeech.auth_key'].length > 0) {
        allModels.push({
          provider: 'salutespeech' as const,
          name: SALUTE_MODEL_NAME,
          displayName: 'SaluteSpeech (Sber cloud)',
          size_mb: 0,
        });
      }
    } catch (err) {
      console.error('Failed to fetch SaluteSpeech settings:', err);
    }

    // Fetch Whisper models
    try {
      const whisperModels = await invoke<RawModelInfo[]>('whisper_get_available_models');
      const availableWhisper = whisperModels
        .filter((m) => m.status === 'Available')
        .map((m) => ({
          provider: 'whisper' as const,
          name: m.name,
          displayName: `${t('Whisper')}: ${m.name}`,
          size_mb: m.size_mb,
        }));
      allModels.push(...availableWhisper);
    } catch (err) {
      console.error('Failed to fetch Whisper models:', err);
    }

    // Fetch Parakeet models
    try {
      const parakeetModels = await invoke<RawModelInfo[]>('parakeet_get_available_models');
      const availableParakeet = parakeetModels
        .filter((m) => m.status === 'Available')
        .map((m) => ({
          provider: 'parakeet' as const,
          name: m.name,
          displayName: `${t('Parakeet')}: ${m.name}`,
          size_mb: m.size_mb,
        }));
      allModels.push(...availableParakeet);
    } catch (err) {
      console.error('Failed to fetch Parakeet models:', err);
    }

    setAvailableModels(allModels);

    // Set default model based on user's saved configuration
    const configuredProvider = transcriptModelConfig?.provider || '';
    const configuredModel = transcriptModelConfig?.model || '';

    // Try to match the configured model.
    // Note: 'localWhisper' in config maps to 'whisper' provider in the model list;
    // GigaAM has a single model, so it matches on provider alone.
    const configuredMatch = allModels.find(
      (m) =>
        (configuredProvider === 'localWhisper' && m.provider === 'whisper' && m.name === configuredModel) ||
        (configuredProvider === 'parakeet' && m.provider === 'parakeet' && m.name === configuredModel) ||
        (configuredProvider === 'gigaam' && m.provider === 'gigaam') ||
        (configuredProvider === 'salutespeech' && m.provider === 'salutespeech')
    );

    // Only set default model if user hasn't manually selected one
    if (!userSelectedRef.current) {
      if (configuredMatch) {
        // Use the configured model if available
        setSelectedModelKey(`${configuredMatch.provider}:${configuredMatch.name}`);
      } else if (allModels.length > 0) {
        // Fall back to first available model
        setSelectedModelKey(`${allModels[0].provider}:${allModels[0].name}`);
      }
    }

    setLoadingModels(false);
  }, [transcriptModelConfig, t]);

  // Reset user selection tracking (call when dialog opens fresh)
  const resetSelection = useCallback(() => {
    userSelectedRef.current = false;
  }, []);

  return {
    availableModels,
    selectedModelKey,
    setSelectedModelKey: setSelectedModelKeyWithTracking,
    loadingModels,
    fetchModels,
    resetSelection,
  };
}
