import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CheckCircle2, Download, FolderOpen, Loader2, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import {
  SenseVoiceAPI,
  SenseVoiceModelInfo,
  senseVoiceDownloadProgress,
} from '@/lib/senseVoice';
import { Button } from './ui/button';
import { LanguageSelection } from './LanguageSelection';

interface SenseVoiceModelManagerProps {
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

export function SenseVoiceModelManager({
  selectedModel,
  onModelSelect,
  autoSave = false,
}: SenseVoiceModelManagerProps) {
  const { setSelectedLanguage } = useConfig();
  const [model, setModel] = useState<SenseVoiceModelInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [preparing, setPreparing] = useState(false);
  const [download, setDownload] = useState<DownloadEvent | null>(null);
  const callbackRef = useRef(onModelSelect);

  useEffect(() => {
    callbackRef.current = onModelSelect;
  }, [onModelSelect]);

  const refresh = useCallback(async () => {
    await SenseVoiceAPI.init();
    const models = await SenseVoiceAPI.getAvailableModels();
    setModel(models[0] ?? null);
  }, []);

  useEffect(() => {
    refresh()
      .catch((error) => toast.error('Failed to inspect SenseVoice model', {
        description: String(error),
      }))
      .finally(() => setLoading(false));
  }, [refresh]);

  useEffect(() => {
    const unlisten = Promise.all([
      listen<DownloadEvent>('sense-voice-model-download-progress', ({ payload }) => {
        setDownload(payload);
        setModel((current) => current && ({
          ...current,
          status: { Downloading: { progress: payload.progress } },
        }));
      }),
      listen<{ modelName: string }>('sense-voice-model-download-complete', async ({ payload }) => {
        setDownload(null);
        await refresh();
        callbackRef.current?.(payload.modelName);
        setSelectedLanguage('auto');
        if (autoSave) {
          await invoke('api_save_transcript_config', {
            provider: 'senseVoice',
            model: payload.modelName,
            apiKey: null,
          });
        }
        toast.success('SenseVoice is ready');
      }),
      listen<{ error: string }>('sense-voice-model-download-error', ({ payload }) => {
        setDownload(null);
        setModel((current) => current && ({
          ...current,
          status: { Error: payload.error },
        }));
        if (!payload.error.toLowerCase().includes('cancelled')) {
          toast.error('SenseVoice download failed', { description: payload.error });
        }
      }),
      listen<string>('sense-voice-model-loading-started', () => setPreparing(true)),
      listen<{ error?: string }>('sense-voice-model-loading-completed', () => setPreparing(false)),
      listen<{ error?: string }>('sense-voice-model-loading-failed', ({ payload }) => {
        setPreparing(false);
        toast.error('SenseVoice preparation failed', { description: payload.error });
      }),
    ]);
    return () => {
      unlisten.then((callbacks) => callbacks.forEach((callback) => callback()));
    };
  }, [autoSave, refresh, setSelectedLanguage]);

  const selectModel = async () => {
    if (!model || model.status !== 'Available') return;
    callbackRef.current?.(model.name);
    setSelectedLanguage('auto');
    if (autoSave) {
      await invoke('api_save_transcript_config', {
        provider: 'senseVoice',
        model: model.name,
        apiKey: null,
      });
    }
  };

  const startDownload = async () => {
    if (!model) return;
    setDownload({
      modelName: model.name,
      progress: 0,
      downloaded_mb: 0,
      total_mb: model.size_mb,
      speed_mbps: 0,
    });
    try {
      await SenseVoiceAPI.downloadModel(model.name);
    } catch {
      // Backend events carry the actionable download error.
    }
  };

  const cancelDownload = async () => {
    await SenseVoiceAPI.cancelDownload();
    setDownload(null);
    await refresh();
  };

  const deleteModel = async () => {
    await SenseVoiceAPI.deleteModel();
    setDownload(null);
    await refresh();
  };

  if (loading) {
    return <div className="flex h-28 items-center justify-center"><Loader2 className="h-5 w-5 animate-spin" /></div>;
  }
  if (!model) {
    return <p className="text-sm text-red-600">SenseVoice model metadata is unavailable.</p>;
  }

  const progress = download?.progress ?? senseVoiceDownloadProgress(model.status);
  const available = model.status === 'Available';
  const selected = available && selectedModel === model.name;
  const failed = typeof model.status === 'object' && ('Error' in model.status || 'Corrupted' in model.status);

  return (
    <div
      className={`border p-4 transition-colors ${selected ? 'border-blue-500 bg-blue-50' : 'border-gray-200 bg-white'} ${available ? 'cursor-pointer' : ''}`}
      onClick={selectModel}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span aria-hidden="true" className="shrink-0 text-2xl leading-none">🗣️</span>
            <h3 className="text-sm font-semibold text-gray-900">SenseVoice Small Int8</h3>
            {selected && <CheckCircle2 className="h-4 w-4 text-blue-600" />}
          </div>
          <p className="ml-8 mt-1 text-sm text-gray-600">Fast automatic recognition for Chinese, Cantonese, English, Japanese, and Korean</p>
          <p className="ml-8 mt-2 text-xs text-gray-500">228 MiB · Apple Neural Engine on Apple Silicon · CPU elsewhere</p>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {available ? (
            <>
              <span className="mr-2 text-xs font-medium text-green-700">
                {preparing ? <span className="flex items-center gap-1"><Loader2 className="h-3 w-3 animate-spin" />Preparing</span> : 'Ready'}
              </span>
              <Button variant="ghost" size="icon" title="Open model folder" onClick={(event) => {
                event.stopPropagation();
                SenseVoiceAPI.openModelsFolder();
              }}><FolderOpen className="h-4 w-4" /></Button>
              <Button variant="ghost" size="icon" title="Delete model" onClick={(event) => {
                event.stopPropagation();
                deleteModel();
              }}><Trash2 className="h-4 w-4 text-red-600" /></Button>
            </>
          ) : progress === null ? (
            <Button size="sm" onClick={(event) => {
              event.stopPropagation();
              startDownload();
            }}>
              <Download className="mr-2 h-4 w-4" />{failed ? 'Download again' : 'Download'}
            </Button>
          ) : (
            <Button variant="ghost" size="icon" title="Cancel download" onClick={(event) => {
              event.stopPropagation();
              cancelDownload();
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

      {available && (
        <div className="mt-4 border-t border-gray-200 pt-4" onClick={(event) => event.stopPropagation()}>
          <LanguageSelection
            selectedLanguage="auto"
            onLanguageChange={setSelectedLanguage}
            provider="senseVoice"
            model={model.name}
          />
        </div>
      )}
    </div>
  );
}
