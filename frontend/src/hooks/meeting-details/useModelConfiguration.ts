import { useState, useEffect, useCallback } from 'react';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface UseModelConfigurationProps {
  serverAddress: string | null;
}

export function useModelConfiguration({ serverAddress }: UseModelConfigurationProps) {
  // Note: No hardcoded defaults - DB is the source of truth
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: '', // Empty until loaded from DB
    whisperModel: 'large-v3'
  });
  const [isLoading, setIsLoading] = useState(true);
  const [, setError] = useState<string>('');

  // Fetch model configuration on mount and when serverAddress changes
  useEffect(() => {
    const fetchModelConfig = async () => {
      setIsLoading(true);
      try {
        console.log('🔄 Fetching model configuration from database...');
        const data = await invokeTauri('api_get_model_config', {}) as any;
        if (data && data.provider !== null) {
          console.log('✅ Loaded model config from database:', {
            provider: data.provider,
            model: data.model,
            whisperModel: data.whisperModel,
            hasApiKey: !!data.apiKey,
            ollamaEndpoint: data.ollamaEndpoint || 'default'
          });
          // Fetch API key if not included and provider requires it
          if (data.provider !== 'ollama' && data.provider !== 'custom-openai' && !data.apiKey) {
            try {
              const apiKeyData = await invokeTauri('api_get_api_key', {
                provider: data.provider
              }) as string;
              data.apiKey = apiKeyData;
            } catch (err) {
              console.error('Failed to fetch API key:', err);
            }
          }

          // Fetch custom OpenAI config if provider is custom-openai
          if (data.provider === 'custom-openai') {
            try {
              const customConfig = await invokeTauri('api_get_custom_openai_config') as any;
              if (customConfig) {
                data.customOpenAIDisplayName = customConfig.displayName || null;
                data.customOpenAIEndpoint = customConfig.endpoint || null;
                data.customOpenAIModel = customConfig.model || null;
                data.customOpenAIApiKey = customConfig.apiKey || null;
                data.maxTokens = customConfig.maxTokens || null;
                data.temperature = customConfig.temperature || null;
                data.topP = customConfig.topP || null;
                // For custom-openai, model field should match customOpenAIModel
                data.model = customConfig.model || data.model;
                console.log('✅ Loaded custom OpenAI config:', {
                  displayName: customConfig.displayName,
                  endpoint: customConfig.endpoint,
                  model: customConfig.model,
                });
              }
            } catch (err) {
              console.error('Failed to fetch custom OpenAI config:', err);
            }
          }

          setModelConfig(data);
        } else {
          console.warn('⚠️ No model config found in database, using defaults');
        }
      } catch (error) {
        console.error('❌ Failed to fetch model config:', error);
      } finally {
        setIsLoading(false);
        console.log('✅ Model configuration loading complete');
      }
    };

    fetchModelConfig();
  }, [serverAddress]);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('Meeting details received model-config-updated event:', event.payload);
        setModelConfig(event.payload);
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);

  // Save model configuration
  const handleSaveModelConfig = useCallback(async (updatedConfig?: ModelConfig) => {
    try {
      const configToSave = updatedConfig || modelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        whisperModel: configToSave.whisperModel,
        apiKey: configToSave.apiKey ?? null,
        ollamaEndpoint: configToSave.ollamaEndpoint ?? null
      };
      console.log('Saving model config with payload:', payload);

      await invokeTauri('api_save_model_config', {
        provider: payload.provider,
        model: payload.model,
        whisperModel: payload.whisperModel,
        apiKey: payload.apiKey,
        ollamaEndpoint: payload.ollamaEndpoint,
      });

      console.log('Save model config success');
      setModelConfig(payload);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', payload);

      toast.success("Summary settings Saved successfully");
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error("Failed to save summary settings", { description: String(error) });
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save model config: Unknown error');
      }
    }
  }, [modelConfig]);

  return {
    modelConfig,
    setModelConfig,
    handleSaveModelConfig,
    isLoading,
  };
}
