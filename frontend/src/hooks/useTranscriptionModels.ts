import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getModelLabel } from '@/lib/transcribe';
import { LOCAL_PROVIDER } from '@/constants/modelDefaults';

export interface RawModelInfo {
  name: string;
  size_mb: number;
  status: 'Available' | 'Missing' | { Downloading: { progress: number } } | { Error: string };
}

export interface ModelOption {
  provider: typeof LOCAL_PROVIDER;
  name: string;
  displayName: string;
  size_mb: number;
}

interface TranscriptModelConfig {
  provider?: string;
  model?: string;
}

/**
 * Fetch downloaded transcription models for the import and retranscribe dialogs.
 *
 * Previously queried two engines and merged the results; there is one engine and
 * one catalog now, so this is a single call.
 */
export function useTranscriptionModels(transcriptModelConfig: TranscriptModelConfig | undefined) {
  const [availableModels, setAvailableModels] = useState<ModelOption[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState<string>('');
  const [loadingModels, setLoadingModels] = useState(false);
  // Track whether the user has manually changed the model selection
  const userSelectedRef = useRef(false);

  const setSelectedModelKeyWithTracking = useCallback((key: string) => {
    userSelectedRef.current = true;
    setSelectedModelKey(key);
  }, []);

  const fetchModels = useCallback(async () => {
    setLoadingModels(true);
    let allModels: ModelOption[] = [];

    try {
      const models = await invoke<RawModelInfo[]>('transcribe_get_available_models');
      allModels = models
        .filter((m) => m.status === 'Available')
        .map((m) => ({
          provider: LOCAL_PROVIDER,
          name: m.name,
          displayName: getModelLabel(m.name),
          size_mb: m.size_mb,
        }));
    } catch (err) {
      console.error('Failed to fetch transcription models:', err);
    }

    setAvailableModels(allModels);

    const configuredModel = transcriptModelConfig?.model || '';
    const configuredMatch = allModels.find((m) => m.name === configuredModel);

    if (!userSelectedRef.current) {
      const pick = configuredMatch ?? allModels[0];
      if (pick) {
        setSelectedModelKey(`${pick.provider}:${pick.name}`);
      }
    }

    setLoadingModels(false);
  }, [transcriptModelConfig]);

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
