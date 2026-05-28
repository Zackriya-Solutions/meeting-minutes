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
    summaryChunkSystemPrompt: '',
    summaryChunkPrompt: '',
    summaryCombineSystemPrompt: '',
    summaryCombinePrompt: '',
    apiKey: null,
    ollamaEndpoint: null
  });
  const [defaultSummaryPrompts, setDefaultSummaryPrompts] = useState({
    system: '',
    chunkSystem: '',
    chunk: '',
    combineSystem: '',
    combine: '',
  });
  const [isSavingSummaryPrompt, setIsSavingSummaryPrompt] = useState(false);

  const { isAutoSummary, toggleIsAutoSummary } = useConfig();

  // Reusable fetch function
  const fetchModelConfig = useCallback(async () => {
    try {
      const [
        defaultSystemPrompt,
        defaultChunkSystemPrompt,
        defaultChunkPrompt,
        defaultCombineSystemPrompt,
        defaultCombinePrompt,
      ] = await Promise.all([
        invoke<string>('api_get_default_summary_system_prompt'),
        invoke<string>('api_get_default_summary_chunk_system_prompt'),
        invoke<string>('api_get_default_summary_chunk_prompt'),
        invoke<string>('api_get_default_summary_combine_system_prompt'),
        invoke<string>('api_get_default_summary_combine_prompt'),
      ]);
      const defaults = {
        system: defaultSystemPrompt,
        chunkSystem: defaultChunkSystemPrompt,
        chunk: defaultChunkPrompt,
        combineSystem: defaultCombineSystemPrompt,
        combine: defaultCombinePrompt,
      };
      setDefaultSummaryPrompts(defaults);

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
          summarySystemPrompt: data.summarySystemPrompt || defaults.system,
          summaryChunkSystemPrompt: data.summaryChunkSystemPrompt || defaults.chunkSystem,
          summaryChunkPrompt: data.summaryChunkPrompt || defaults.chunk,
          summaryCombineSystemPrompt: data.summaryCombineSystemPrompt || defaults.combineSystem,
          summaryCombinePrompt: data.summaryCombinePrompt || defaults.combine,
        });
      } else {
        setModelConfig((prev) => ({
          ...prev,
          summarySystemPrompt: prev.summarySystemPrompt || defaults.system,
          summaryChunkSystemPrompt: prev.summaryChunkSystemPrompt || defaults.chunkSystem,
          summaryChunkPrompt: prev.summaryChunkPrompt || defaults.chunk,
          summaryCombineSystemPrompt: prev.summaryCombineSystemPrompt || defaults.combineSystem,
          summaryCombinePrompt: prev.summaryCombinePrompt || defaults.combine,
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
          summarySystemPrompt: event.payload.summarySystemPrompt || defaultSummaryPrompts.system,
          summaryChunkSystemPrompt: event.payload.summaryChunkSystemPrompt || defaultSummaryPrompts.chunkSystem,
          summaryChunkPrompt: event.payload.summaryChunkPrompt || defaultSummaryPrompts.chunk,
          summaryCombineSystemPrompt: event.payload.summaryCombineSystemPrompt || defaultSummaryPrompts.combineSystem,
          summaryCombinePrompt: event.payload.summaryCombinePrompt || defaultSummaryPrompts.combine,
        });
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, [defaultSummaryPrompts]);

  // Save handler
  const handleSaveModelConfig = async (config: ModelConfig, successMessage = 'Model settings saved successfully') => {
    try {
      const configToSave = {
        ...config,
        summarySystemPrompt: config.summarySystemPrompt?.trim() || defaultSummaryPrompts.system,
        summaryChunkSystemPrompt: config.summaryChunkSystemPrompt?.trim() || defaultSummaryPrompts.chunkSystem,
        summaryChunkPrompt: config.summaryChunkPrompt?.trim() || defaultSummaryPrompts.chunk,
        summaryCombineSystemPrompt: config.summaryCombineSystemPrompt?.trim() || defaultSummaryPrompts.combineSystem,
        summaryCombinePrompt: config.summaryCombinePrompt?.trim() || defaultSummaryPrompts.combine,
      };

      await invoke('api_save_model_config', {
        provider: configToSave.provider,
        model: configToSave.model,
        whisperModel: configToSave.whisperModel,
        apiKey: configToSave.apiKey,
        ollamaEndpoint: configToSave.ollamaEndpoint,
        summarySystemPrompt: configToSave.summarySystemPrompt,
        summaryChunkSystemPrompt: configToSave.summaryChunkSystemPrompt,
        summaryChunkPrompt: configToSave.summaryChunkPrompt,
        summaryCombineSystemPrompt: configToSave.summaryCombineSystemPrompt,
        summaryCombinePrompt: configToSave.summaryCombinePrompt,
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

  const handleSaveSummaryPrompts = async () => {
    setIsSavingSummaryPrompt(true);
    try {
      await handleSaveModelConfig(modelConfig, 'Summary prompts saved successfully');
    } finally {
      setIsSavingSummaryPrompt(false);
    }
  };

  const handleResetSummaryPrompts = async () => {
    if (
      !defaultSummaryPrompts.system ||
      !defaultSummaryPrompts.chunkSystem ||
      !defaultSummaryPrompts.chunk ||
      !defaultSummaryPrompts.combineSystem ||
      !defaultSummaryPrompts.combine
    ) return;

    setIsSavingSummaryPrompt(true);
    try {
      await handleSaveModelConfig(
        {
          ...modelConfig,
          summarySystemPrompt: defaultSummaryPrompts.system,
          summaryChunkSystemPrompt: defaultSummaryPrompts.chunkSystem,
          summaryChunkPrompt: defaultSummaryPrompts.chunk,
          summaryCombineSystemPrompt: defaultSummaryPrompts.combineSystem,
          summaryCombinePrompt: defaultSummaryPrompts.combine,
        },
        'Summary prompts reset to default'
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
        <h3 className="text-lg font-semibold mb-4">Summary Prompts</h3>
        <p className="text-sm text-gray-600 mb-4">
          Customize the prompts used by Generate Summary. The chunk and combine prompts are used only for
          long transcripts that need multi-step summarization.
        </p>

        <div className="space-y-6">
          <div>
            <h4 className="text-sm font-medium text-gray-900 mb-2">Final system prompt</h4>
            <p className="text-xs text-gray-600 mb-2">
              Keep
              <code className="mx-1 rounded bg-gray-100 px-1 py-0.5">{'{{SECTION_INSTRUCTIONS}}'}</code>
              and
              <code className="mx-1 rounded bg-gray-100 px-1 py-0.5">{'{{TEMPLATE}}'}</code>
              to preserve the selected summary template.
            </p>
            <Textarea
              value={modelConfig.summarySystemPrompt}
              onChange={(event) => setModelConfig((prev) => ({
                ...prev,
                summarySystemPrompt: event.target.value,
              }))}
              className="min-h-[260px] font-mono text-xs leading-relaxed"
              placeholder={defaultSummaryPrompts.system || 'Loading default final system prompt...'}
            />
          </div>

          <div>
            <h4 className="text-sm font-medium text-gray-900 mb-2">Chunk system prompt</h4>
            <p className="text-xs text-gray-600 mb-2">
              System role used when summarizing each transcript chunk.
            </p>
            <Textarea
              value={modelConfig.summaryChunkSystemPrompt}
              onChange={(event) => setModelConfig((prev) => ({
                ...prev,
                summaryChunkSystemPrompt: event.target.value,
              }))}
              className="min-h-[80px] font-mono text-xs leading-relaxed"
              placeholder={defaultSummaryPrompts.chunkSystem || 'Loading default chunk system prompt...'}
            />
          </div>

          <div>
            <h4 className="text-sm font-medium text-gray-900 mb-2">Chunk user prompt</h4>
            <p className="text-xs text-gray-600 mb-2">
              Used to summarize each transcript chunk. Keep
              <code className="mx-1 rounded bg-gray-100 px-1 py-0.5">{'{{TRANSCRIPT_CHUNK}}'}</code>
              where the chunk text should be inserted.
            </p>
            <Textarea
              value={modelConfig.summaryChunkPrompt}
              onChange={(event) => setModelConfig((prev) => ({
                ...prev,
                summaryChunkPrompt: event.target.value,
              }))}
              className="min-h-[180px] font-mono text-xs leading-relaxed"
              placeholder={defaultSummaryPrompts.chunk || 'Loading default chunk prompt...'}
            />
          </div>

          <div>
            <h4 className="text-sm font-medium text-gray-900 mb-2">Combine system prompt</h4>
            <p className="text-xs text-gray-600 mb-2">
              System role used when merging intermediate chunk summaries.
            </p>
            <Textarea
              value={modelConfig.summaryCombineSystemPrompt}
              onChange={(event) => setModelConfig((prev) => ({
                ...prev,
                summaryCombineSystemPrompt: event.target.value,
              }))}
              className="min-h-[80px] font-mono text-xs leading-relaxed"
              placeholder={defaultSummaryPrompts.combineSystem || 'Loading default combine system prompt...'}
            />
          </div>

          <div>
            <h4 className="text-sm font-medium text-gray-900 mb-2">Combine user prompt</h4>
            <p className="text-xs text-gray-600 mb-2">
              Used to merge chunk summaries before the final report. Keep
              <code className="mx-1 rounded bg-gray-100 px-1 py-0.5">{'{{CHUNK_SUMMARIES}}'}</code>
              where the intermediate summaries should be inserted.
            </p>
            <Textarea
              value={modelConfig.summaryCombinePrompt}
              onChange={(event) => setModelConfig((prev) => ({
                ...prev,
                summaryCombinePrompt: event.target.value,
              }))}
              className="min-h-[180px] font-mono text-xs leading-relaxed"
              placeholder={defaultSummaryPrompts.combine || 'Loading default combine prompt...'}
            />
          </div>
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={handleResetSummaryPrompts}
            disabled={
              isSavingSummaryPrompt ||
              !defaultSummaryPrompts.system ||
              !defaultSummaryPrompts.chunkSystem ||
              !defaultSummaryPrompts.chunk ||
              !defaultSummaryPrompts.combineSystem ||
              !defaultSummaryPrompts.combine
            }
          >
            Reset prompts to default
          </Button>
          <Button
            type="button"
            onClick={handleSaveSummaryPrompts}
            disabled={isSavingSummaryPrompt}
          >
            {isSavingSummaryPrompt ? 'Saving...' : 'Save prompts'}
          </Button>
        </div>
      </div>
    </div>
  );
}
