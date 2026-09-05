import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CheckCircle2, Download, FolderOpen, Loader2, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';
import {
  QwenAsrAPI,
  QwenModelInfo,
  QWEN_MODEL_DISPLAY,
  qwenDownloadProgress,
} from '@/lib/qwenAsr';
import { Button } from './ui/button';
import { LanguageSelection } from './LanguageSelection';
import { useConfig } from '@/contexts/ConfigContext';
import { LANGUAGES } from '@/constants/languages';
import {
  getTranscriptionLanguageCapability,
  supportsTranscriptionLanguage,
} from '@/lib/parakeet';

interface QwenAsrModelManagerProps {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
  autoSave?: boolean;
}

interface DownloadEvent {
  modelName: string;
  progress: number;
  downloaded_mb: number;
  total_mb: number;
  speed_mbps: number;
}

interface DownloadErrorEvent {
  modelName: string;
  error: string;
}

export function QwenAsrModelManager({
  selectedModel,
  onModelSelect,
  autoSave = false,
}: QwenAsrModelManagerProps) {
  const { selectedLanguage, setSelectedLanguage } = useConfig();
  const [models, setModels] = useState<QwenModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [downloads, setDownloads] = useState<Record<string, DownloadEvent>>({});
  const callbackRef = useRef(onModelSelect);

  useEffect(() => {
    callbackRef.current = onModelSelect;
  }, [onModelSelect]);

  const refresh = useCallback(async () => {
    await QwenAsrAPI.init();
    setModels(await QwenAsrAPI.getAvailableModels());
  }, []);

  useEffect(() => {
    refresh()
      .catch((error) => toast.error('Failed to inspect Qwen3-ASR models', {
        description: String(error),
      }))
      .finally(() => setLoading(false));
  }, [refresh]);

  useEffect(() => {
    const removeDownload = (modelName: string) => {
      setDownloads((current) => {
        const next = { ...current };
        delete next[modelName];
        return next;
      });
    };

    const unlisten = Promise.all([
      listen<DownloadEvent>('qwen-asr-model-download-progress', ({ payload }) => {
        setDownloads((current) => ({ ...current, [payload.modelName]: payload }));
        setModels((current) => current.map((model) => model.name === payload.modelName
          ? { ...model, status: { Downloading: { progress: payload.progress } } }
          : model));
      }),
      listen<{ modelName: string }>('qwen-asr-model-download-complete', async ({ payload }) => {
        removeDownload(payload.modelName);
        await refresh();
        callbackRef.current?.(payload.modelName);
        if (autoSave) {
          await invoke('api_save_transcript_config', {
            provider: 'qwen3Asr',
            model: payload.modelName,
            apiKey: null,
          });
        }
        toast.success(`${QWEN_MODEL_DISPLAY[payload.modelName]?.name ?? payload.modelName} is ready`);
      }),
      listen<DownloadErrorEvent>('qwen-asr-model-download-error', ({ payload }) => {
        removeDownload(payload.modelName);
        setModels((current) => current.map((model) => model.name === payload.modelName
          ? { ...model, status: { Error: payload.error } }
          : model));
        if (!payload.error.toLowerCase().includes('cancelled')) {
          toast.error('Qwen3-ASR download failed', { description: payload.error });
        }
      }),
    ]);
    return () => {
      unlisten.then((callbacks) => callbacks.forEach((callback) => callback()));
    };
  }, [autoSave, refresh]);

  const selectModel = async (model: QwenModelInfo) => {
    if (model.status !== 'Available') return;
    callbackRef.current?.(model.name);
    if (autoSave) {
      await invoke('api_save_transcript_config', {
        provider: 'qwen3Asr',
        model: model.name,
        apiKey: null,
      });
    }
  };

  const startDownload = async (model: QwenModelInfo) => {
    setDownloads((current) => ({
      ...current,
      [model.name]: {
        modelName: model.name,
        progress: 0,
        downloaded_mb: 0,
        total_mb: model.size_mb,
        speed_mbps: 0,
      },
    }));
    try {
      await QwenAsrAPI.downloadModel(model.name);
    } catch {
      // The backend event contains the actionable error and updates the card.
    }
  };

  const cancelDownload = async (modelName: string) => {
    await QwenAsrAPI.cancelDownload();
    setDownloads((current) => {
      const next = { ...current };
      delete next[modelName];
      return next;
    });
    await refresh();
  };

  const deleteModel = async (modelName: string) => {
    await QwenAsrAPI.deleteModel(modelName);
    await refresh();
  };

  if (loading) {
    return <div className="flex h-28 items-center justify-center"><Loader2 className="h-5 w-5 animate-spin" /></div>;
  }
  if (models.length === 0) {
    return <p className="text-sm text-red-600">Qwen3-ASR model metadata is unavailable.</p>;
  }

  const languageCapability = getTranscriptionLanguageCapability('qwen3Asr', selectedModel);
  const effectiveLanguage = supportsTranscriptionLanguage(languageCapability, selectedLanguage)
    ? selectedLanguage
    : 'auto';
  const languageName = LANGUAGES.find(({ code }) => code === effectiveLanguage)?.name
    ?? 'Auto Detect (Original Language)';

  return (
    <div className="space-y-3">
      {models.map((model) => {
        const display = QWEN_MODEL_DISPLAY[model.name] ?? {
          name: model.name,
          icon: '🎙️',
          tagline: model.description,
        };
        const download = downloads[model.name];
        const progress = download?.progress ?? qwenDownloadProgress(model.status);
        const available = model.status === 'Available';
        const selected = available && selectedModel === model.name;
        const failed = typeof model.status === 'object'
          && ('Error' in model.status || 'Corrupted' in model.status);

        return (
          <div
            key={model.name}
            className={`border p-4 transition-colors ${selected ? 'border-blue-500 bg-blue-50' : 'border-gray-200 bg-white'} ${available ? 'cursor-pointer' : ''}`}
            onClick={() => selectModel(model)}
          >
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span aria-hidden="true" className="shrink-0 text-2xl leading-none">
                    {display.icon}
                  </span>
                  <h3 className="text-sm font-semibold text-gray-900">{display.name}</h3>
                  {display.recommended && (
                    <span className="bg-blue-100 px-1.5 py-0.5 text-xs font-medium text-blue-700">Recommended</span>
                  )}
                  {selected && <CheckCircle2 className="h-4 w-4 text-blue-600" />}
                </div>
                <p className="ml-8 mt-1 text-sm text-gray-600">{display.tagline}</p>
                <p className="ml-8 mt-2 text-xs text-gray-500">
                  {model.size_mb.toLocaleString()} MiB · CPU · Recognition: {languageName}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-1">
                {available ? (
                  <>
                    <span className="mr-2 text-xs font-medium text-green-700">Ready</span>
                    <Button variant="ghost" size="icon" title="Open model folder" onClick={(event) => {
                      event.stopPropagation();
                      QwenAsrAPI.openModelsFolder();
                    }}><FolderOpen className="h-4 w-4" /></Button>
                    <Button variant="ghost" size="icon" title={`Delete ${display.name}`} onClick={(event) => {
                      event.stopPropagation();
                      deleteModel(model.name);
                    }}><Trash2 className="h-4 w-4 text-red-600" /></Button>
                  </>
                ) : progress === null ? (
                  <Button size="sm" onClick={(event) => {
                    event.stopPropagation();
                    startDownload(model);
                  }}>
                    <Download className="mr-2 h-4 w-4" />{failed ? 'Download again' : 'Download'}
                  </Button>
                ) : (
                  <Button variant="ghost" size="icon" title={`Cancel ${display.name} download`} onClick={(event) => {
                    event.stopPropagation();
                    cancelDownload(model.name);
                  }}><X className="h-4 w-4" /></Button>
                )}
              </div>
            </div>

            {progress !== null && (
              <div className="mt-4 border-t border-gray-200 pt-3">
                <div className="mb-2 flex items-center justify-between text-xs text-gray-600">
                  <span>Downloading {download?.downloaded_mb.toFixed(1) ?? '0.0'} / {download?.total_mb.toFixed(1) ?? model.size_mb} MiB</span>
                  <span>{progress}%{download && download.speed_mbps > 0 ? ` · ${download.speed_mbps.toFixed(1)} MiB/s` : ''}</span>
                </div>
                <div className="h-2 overflow-hidden bg-gray-200">
                  <div className="h-full bg-blue-600 transition-[width]" style={{ width: `${progress}%` }} />
                </div>
              </div>
            )}

            {selected && (
              <div
                className="mt-4 border-t border-gray-200 pt-4"
                onClick={(event) => event.stopPropagation()}
              >
                <LanguageSelection
                  selectedLanguage={effectiveLanguage}
                  onLanguageChange={setSelectedLanguage}
                  provider="qwen3Asr"
                  model={model.name}
                />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
