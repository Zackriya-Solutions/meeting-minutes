'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Sparkles } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import { createDebouncedSaver } from '@/lib/debouncedSaver';
import {
  SUMMARY_CONTEXT_SAVE_DELAY_MS,
  SUMMARY_CONTEXT_TEXTAREA_CLASS,
} from '@/lib/summary-context';

export const CLEANUP_PROMPT_MAX_CHARS = 8000;

interface CleanupConfigResponse {
  enabled: boolean;
  provider: string;
  model: string;
  ollamaEndpoint?: string | null;
  prompt: string;
}

const EMPTY_MODEL: ModelConfig = {
  provider: 'builtin-ai',
  model: 'qwen3.5:2b',
  whisperModel: 'large-v3',
  apiKey: null,
  ollamaEndpoint: null,
};

/** Post-transcription cleanup controls. The switch defaults to off. */
export function TranscriptCleanupSettings() {
  const [enabled, setEnabled] = useState(false);
  const [prompt, setPrompt] = useState('');
  const [modelConfig, setModelConfig] = useState<ModelConfig>(EMPTY_MODEL);
  const [isLoaded, setIsLoaded] = useState(false);
  const enabledRef = useRef(enabled);
  const promptRef = useRef(prompt);
  const modelRef = useRef(modelConfig);
  enabledRef.current = enabled;
  promptRef.current = prompt;
  modelRef.current = modelConfig;

  const persist = useCallback(async (
    nextEnabled: boolean,
    nextPrompt: string,
    nextModel: ModelConfig,
  ) => {
    await invoke('api_save_cleanup_config', {
      enabled: nextEnabled,
      provider: nextModel.provider,
      model: nextModel.model,
      ollamaEndpoint: nextModel.ollamaEndpoint ?? null,
      prompt: nextPrompt,
      apiKey: nextModel.apiKey ?? null,
    });
  }, []);

  const [promptSaver] = useState(() => createDebouncedSaver<string>(
    (_key, value) => persist(enabledRef.current, value, modelRef.current),
    (error) => {
      console.error('Failed to save transcript cleanup settings:', error);
      toast.error('Failed to save transcript cleanup settings', { description: String(error) });
    },
    SUMMARY_CONTEXT_SAVE_DELAY_MS,
  ));

  useEffect(() => {
    let cancelled = false;
    invoke<CleanupConfigResponse>('api_get_cleanup_config')
      .then(saved => {
        if (cancelled) return;
        setEnabled(saved.enabled);
        setPrompt(saved.prompt ?? '');
        setModelConfig({
          ...EMPTY_MODEL,
          provider: (saved.provider || EMPTY_MODEL.provider) as ModelConfig['provider'],
          model: saved.model || EMPTY_MODEL.model,
          ollamaEndpoint: saved.ollamaEndpoint ?? null,
        });
        setIsLoaded(true);
      })
      .catch(error => {
        console.error('Failed to load transcript cleanup settings:', error);
        toast.error('Failed to load transcript cleanup settings', { description: String(error) });
      });
    return () => {
      cancelled = true;
      void promptSaver.flush();
    };
  }, [promptSaver]);

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2 mb-2">
            <Sparkles size={18} className="text-gray-500" />
            <h3 className="text-lg font-semibold text-gray-900">Transcript cleanup</h3>
          </div>
          <p className="text-sm text-gray-600">
            After transcription, correct names and terms with the prompt below. Off leaves the transcript as the speech engine wrote it.
          </p>
        </div>
        <Switch
          checked={enabled}
          disabled={!isLoaded}
          onCheckedChange={(checked) => {
            setEnabled(checked);
            enabledRef.current = checked;
            void promptSaver.flush().then(() => persist(checked, promptRef.current, modelRef.current)).catch(error => {
              console.error('Failed to save transcript cleanup settings:', error);
              toast.error('Failed to save transcript cleanup settings', { description: String(error) });
            });
          }}
          aria-label="Clean up the transcript after transcription"
        />
      </div>

      <textarea
        aria-label="Transcript cleanup prompt"
        className={`${SUMMARY_CONTEXT_TEXTAREA_CLASS} min-h-[120px]`}
        value={prompt}
        maxLength={CLEANUP_PROMPT_MAX_CHARS}
        disabled={!isLoaded}
        placeholder="Names, products, and spellings to prefer. For example Elasticsearch, OpenTelemetry, anomaly detection."
        onChange={(event) => {
          const next = event.target.value;
          promptRef.current = next;
          setPrompt(next);
          promptSaver.schedule('cleanup-prompt', next);
        }}
      />
      <div className="text-right text-xs text-gray-500">
        {prompt.length.toLocaleString('en-US')} / {CLEANUP_PROMPT_MAX_CHARS.toLocaleString('en-US')}
      </div>

      <div>
        <h4 className="text-sm font-medium text-gray-900 mb-1">Cleanup model</h4>
        <p className="text-sm text-gray-600 mb-4">
          The model used for the cleanup pass. This does not change the summary model.
        </p>
        <ModelSettingsModal
          modelConfig={modelConfig}
          setModelConfig={setModelConfig}
          skipInitialFetch
          onSave={(config) => {
            setModelConfig(config);
            modelRef.current = config;
            void promptSaver.flush().then(() => persist(enabledRef.current, promptRef.current, config)).catch(error => {
              console.error('Failed to save transcript cleanup settings:', error);
              toast.error('Failed to save transcript cleanup settings', { description: String(error) });
            });
          }}
        />
      </div>
    </div>
  );
}
