'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import { SummaryLanguageSettings } from '@/components/SummaryLanguageSettings';
import { useT } from '@/lib/i18n';

interface SummaryModelSettingsProps {
  refetchTrigger?: number; // Change this to trigger refetch
}

export function SummaryModelSettings({ refetchTrigger }: SummaryModelSettingsProps) {
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'deepseek',
    model: 'deepseek-v4-pro',
    whisperModel: 'large-v3',
    apiKey: null,
    ollamaEndpoint: null
  });

  const t = useT();

  // Reusable fetch function
  const fetchModelConfig = useCallback(async () => {
    try {
      const data = await invoke('api_get_model_config') as any;
      if (data && data.provider !== null) {
        // Fetch API key if not included and provider requires it.
        // ollama/builtin-ai need none; gigachat/deepseek keep theirs in Settings → Providers.
        const providerHasNoSettingsKey =
          data.provider === 'ollama' ||
          data.provider === 'builtin-ai' ||
          data.provider === 'gigachat' ||
          data.provider === 'deepseek';
        if (!providerHasNoSettingsKey && !data.apiKey) {
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
        setModelConfig(data);
      }
    } catch (error) {
      console.error('Failed to fetch model config:', error);
      toast.error(t('Failed to load model settings'));
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

  // Save handler
  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey,
        ollamaEndpoint: config.ollamaEndpoint,
      });

      setModelConfig(config);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success(t('Model settings saved successfully'));
    } catch (error) {
      console.error('Error saving model config:', error);
      toast.error(t('Failed to save model settings'));
    }
  };

  return (
    <div className='flex flex-col gap-4'>
      <SummaryLanguageSettings />

      <div className="bg-background rounded-lg border border-border p-6 shadow-none" data-summary-model-config>
        <h3 className="text-lg font-semibold mb-4">{t('Summary Model Configuration')}</h3>
        <p className="text-sm text-muted-foreground mb-6">
          {t('Configure the AI model used for generating meeting summaries.')}
        </p>

        {/* Managed pilot: DeepSeek runs through the Memento gateway (no API key). Offer a
            simple quality switch (Pro / Flash). Other providers keep the full modal. */}
        {modelConfig.provider === 'deepseek' ? (
          <DeepSeekModelPicker
            model={modelConfig.model}
            onSelect={(model) =>
              handleSaveModelConfig({ ...modelConfig, provider: 'deepseek', model })
            }
          />
        ) : (
          <ModelSettingsModal
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            onSave={handleSaveModelConfig}
            skipInitialFetch={true}
          />
        )}
      </div>
    </div>
  );
}

/**
 * Quality switch for the managed DeepSeek summary model. Both variants run in the cloud
 * through the Memento gateway (no API key). Pro = higher quality (default), Flash = faster.
 */
function DeepSeekModelPicker({
  model,
  onSelect,
}: {
  model: string;
  onSelect: (model: string) => void;
}) {
  const t = useT();
  const OPTIONS: { id: string; title: string; subtitle: string }[] = [
    {
      id: 'deepseek-v4-pro',
      title: 'DeepSeek v4 Pro',
      subtitle: t('Higher quality and more thorough. Recommended.'),
    },
    {
      id: 'deepseek-v4-flash',
      title: 'DeepSeek v4 Flash',
      subtitle: t('Faster and lighter, with more concise summaries.'),
    },
  ];
  // Treat any unknown/legacy value as Pro so the selection is never empty.
  const active = model === 'deepseek-v4-flash' ? 'deepseek-v4-flash' : 'deepseek-v4-pro';

  return (
    <div className="grid gap-2">
      {OPTIONS.map((opt) => {
        const isActive = active === opt.id;
        return (
          <button
            key={opt.id}
            type="button"
            onClick={() => onSelect(opt.id)}
            className={`mm-press flex items-start gap-3 rounded-lg border p-4 text-left transition-colors ${
              isActive
                ? 'border-primary/40 bg-primary/10'
                : 'border-border bg-card hover:border-border hover:bg-accent'
            }`}
          >
            <span className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${isActive ? 'border-primary' : 'border-border'}`}>
              {isActive && <span className="h-2 w-2 rounded-full bg-primary" />}
            </span>
            <span>
              <span className="block text-sm font-medium text-foreground">{opt.title}</span>
              <span className="mt-0.5 block text-xs text-muted-foreground">{opt.subtitle}</span>
            </span>
          </button>
        );
      })}
      <p className="mt-1 text-xs text-muted-foreground">
        {t('Runs in the cloud through the Memento gateway. No API key required.')}
      </p>
    </div>
  );
}
