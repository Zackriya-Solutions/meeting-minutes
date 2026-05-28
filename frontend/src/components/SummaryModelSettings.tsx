'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import { Switch } from './ui/switch';
import { useConfig } from '@/contexts/ConfigContext';
import { Textarea } from './ui/textarea';
import { Button } from './ui/button';

interface SummaryModelSettingsProps {
  refetchTrigger?: number; // Change this to trigger refetch
}

export function SummaryModelSettings({ refetchTrigger }: SummaryModelSettingsProps) {
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: 'llama3.2:latest',
    whisperModel: 'large-v3',
    summarySystemPrompt: '',
    apiKey: null,
    ollamaEndpoint: null
  });
  const [defaultSummarySystemPrompt, setDefaultSummarySystemPrompt] = useState('');
  const [isSavingSummaryPrompt, setIsSavingSummaryPrompt] = useState(false);

  const { isAutoSummary, toggleIsAutoSummary } = useConfig();

  // Reusable fetch function
  const fetchModelConfig = useCallback(async () => {
    try {
      const defaultPrompt = await invoke<string>('api_get_default_summary_system_prompt');
      setDefaultSummarySystemPrompt(defaultPrompt);

      const data = await invoke('api_get_model_config') as any;
      if (data && data.provider !== null) {
        // Fetch API key if not included and provider requires it
        if (data.provider !== 'ollama' && data.provider !== 'builtin-ai' && !data.apiKey) {
          try {
            const apiKeyData = await invoke('api_get_api_key', {
              provider: data.provider
            }) as string;
            data.apiKey = apiKeyData;
          } catch (err) {
            console.error('Failed to fetch API key:', err);
          }
        }
        // Fetch Custom OpenAI config if that's the active provider
        if (data.provider === 'custom-openai') {
          try {
            const customConfig = (await invoke('api_get_custom_openai_config')) as any;
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
            }
          } catch (err) {
            console.error('Failed to fetch custom OpenAI config:', err);
          }
        }
        setModelConfig({
          ...data,
          summarySystemPrompt: data.summarySystemPrompt || defaultPrompt,
        });
      } else {
        setModelConfig((prev) => ({
          ...prev,
          summarySystemPrompt: prev.summarySystemPrompt || defaultPrompt,
        }));
      }
    } catch (error) {
      console.error('Failed to fetch model config:', error);
      toast.error('Failed to load model settings');
    }
  }, []);

  // Fetch on mount
  useEffect(() => {
    fetchModelConfig();
  }, [fetchModelConfig]);

  // Refetch when trigger changes (optional external control)
  useEffect(() => {
    if (refetchTrigger !== undefined && refetchTrigger > 0) {
      fetchModelConfig();
    }
  }, [refetchTrigger, fetchModelConfig]);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('SummaryModelSettings received model-config-updated event:', event.payload);
        setModelConfig({
          ...event.payload,
          summarySystemPrompt: event.payload.summarySystemPrompt || defaultSummarySystemPrompt,
        });
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, [defaultSummarySystemPrompt]);

  // Save handler
  const handleSaveModelConfig = async (config: ModelConfig, successMessage = 'Model settings saved successfully') => {
    try {
      const configToSave = {
        ...config,
        summarySystemPrompt: config.summarySystemPrompt?.trim() || defaultSummarySystemPrompt,
      };

      await invoke('api_save_model_config', {
        provider: configToSave.provider,
        model: configToSave.model,
        whisperModel: configToSave.whisperModel,
        apiKey: configToSave.apiKey,
        ollamaEndpoint: configToSave.ollamaEndpoint,
        summarySystemPrompt: configToSave.summarySystemPrompt,
      });

      setModelConfig(configToSave);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', configToSave);

      toast.success(successMessage);
    } catch (error) {
      console.error('Error saving model config:', error);
      toast.error('Failed to save model settings');
    }
  };

  const handleSaveSummarySystemPrompt = async () => {
    setIsSavingSummaryPrompt(true);
    try {
      await handleSaveModelConfig(modelConfig, 'Summary system prompt saved successfully');
    } finally {
      setIsSavingSummaryPrompt(false);
    }
  };

  const handleResetSummarySystemPrompt = async () => {
    if (!defaultSummarySystemPrompt) return;

    setIsSavingSummaryPrompt(true);
    try {
      await handleSaveModelConfig(
        { ...modelConfig, summarySystemPrompt: defaultSummarySystemPrompt },
        'Summary system prompt reset to default'
      );
    } finally {
      setIsSavingSummaryPrompt(false);
    }
  };

  return (
    <div className='flex flex-col gap-4'>
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-lg font-semibold text-gray-900 mb-2">Auto Summary</h3>
            <p className="text-sm text-gray-600">Auto Generating summary after meeting completion(Stopping)</p>
          </div>
          <Switch checked={isAutoSummary} onCheckedChange={toggleIsAutoSummary} />
        </div>
      </div>

      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <h3 className="text-lg font-semibold mb-4">Summary Model Configuration</h3>
        <p className="text-sm text-gray-600 mb-6">
          Configure the AI model used for generating meeting summaries.
        </p>

        <ModelSettingsModal
          modelConfig={modelConfig}
          setModelConfig={setModelConfig}
          onSave={handleSaveModelConfig}
          skipInitialFetch={true}
        />
      </div>

      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <h3 className="text-lg font-semibold mb-4">Summary System Prompt</h3>
        <p className="text-sm text-gray-600 mb-4">
          Customize the system prompt used by Generate Summary. Keep the placeholders
          <code className="mx-1 rounded bg-gray-100 px-1 py-0.5 text-xs">{'{{SECTION_INSTRUCTIONS}}'}</code>
          and
          <code className="mx-1 rounded bg-gray-100 px-1 py-0.5 text-xs">{'{{TEMPLATE}}'}</code>
          if you want the selected summary template to remain active.
        </p>
        <Textarea
          value={modelConfig.summarySystemPrompt}
          onChange={(event) => setModelConfig((prev) => ({
            ...prev,
            summarySystemPrompt: event.target.value,
          }))}
          className="min-h-[280px] font-mono text-xs leading-relaxed"
          placeholder={defaultSummarySystemPrompt || 'Loading default summary system prompt...'}
        />
        <div className="mt-4 flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={handleResetSummarySystemPrompt}
            disabled={isSavingSummaryPrompt || !defaultSummarySystemPrompt}
          >
            Reset to default
          </Button>
          <Button
            type="button"
            onClick={handleSaveSummarySystemPrompt}
            disabled={isSavingSummaryPrompt}
          >
            {isSavingSummaryPrompt ? 'Saving...' : 'Save prompt'}
          </Button>
        </div>
      </div>
    </div>
  );
}
