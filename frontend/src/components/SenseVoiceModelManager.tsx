import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CheckCircle2, Download, FolderOpen, Loader2, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { SenseVoiceAPI, SenseVoiceModelInfo, senseVoiceDownloadProgress } from '@/lib/senseVoice';
import { Button } from './ui/button';
import { LanguageSelection } from './LanguageSelection';

interface SenseVoiceModelManagerProps { selectedModel?: string; onModelSelect?: (modelName: string) => void; autoSave?: boolean; }
interface DownloadEvent { modelName: string; progress: number; downloaded_mb: number; total_mb: number; speed_mbps: number; }

export function SenseVoiceModelManager({ selectedModel, onModelSelect, autoSave = false }: SenseVoiceModelManagerProps) {
  const { setSelectedLanguage } = useConfig();
  const [models, setModels] = useState<SenseVoiceModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [preparing, setPreparing] = useState(false);
  const [download, setDownload] = useState<DownloadEvent | null>(null);
  const callbackRef = useRef(onModelSelect);
  useEffect(() => { callbackRef.current = onModelSelect; }, [onModelSelect]);

  const refresh = useCallback(async () => { await SenseVoiceAPI.init(); setModels(await SenseVoiceAPI.getAvailableModels()); }, []);
  useEffect(() => { refresh().catch((error) => toast.error('Failed to inspect SenseVoice models', { description: String(error) })).finally(() => setLoading(false)); }, [refresh]);
  useEffect(() => {
    const unlisten = Promise.all([
      listen<DownloadEvent>('sense-voice-model-download-progress', ({ payload }) => {
        setDownload(payload);
        setModels((current) => current.map((model) => model.name === payload.modelName ? { ...model, status: { Downloading: { progress: payload.progress } } } : model));
      }),
      listen<{ modelName: string }>('sense-voice-model-download-complete', async ({ payload }) => {
        setDownload(null); await refresh(); callbackRef.current?.(payload.modelName); setSelectedLanguage('auto');
        if (autoSave) await invoke('api_save_transcript_config', { provider: 'senseVoice', model: payload.modelName, apiKey: null });
        toast.success('SenseVoice is ready');
      }),
      listen<{ error: string; modelName: string }>('sense-voice-model-download-error', ({ payload }) => {
        setDownload(null);
        setModels((current) => current.map((model) => model.name === payload.modelName ? { ...model, status: { Error: payload.error } } : model));
        if (!payload.error.toLowerCase().includes('cancelled')) toast.error('SenseVoice download failed', { description: payload.error });
      }),
      listen<string>('sense-voice-model-loading-started', () => setPreparing(true)),
      listen('sense-voice-model-loading-completed', () => setPreparing(false)),
      listen<{ error?: string }>('sense-voice-model-loading-failed', ({ payload }) => { setPreparing(false); toast.error('SenseVoice preparation failed', { description: payload.error }); }),
    ]);
    return () => { unlisten.then((callbacks) => callbacks.forEach((callback) => callback())); };
  }, [autoSave, refresh, setSelectedLanguage]);

  const selectModel = async (model: SenseVoiceModelInfo) => { if (model.status !== 'Available') return; callbackRef.current?.(model.name); setSelectedLanguage('auto'); if (autoSave) await invoke('api_save_transcript_config', { provider: 'senseVoice', model: model.name, apiKey: null }); };
  const startDownload = async (model: SenseVoiceModelInfo) => { setDownload({ modelName: model.name, progress: 0, downloaded_mb: 0, total_mb: model.size_mb, speed_mbps: 0 }); try { await SenseVoiceAPI.downloadModel(model.name); } catch { /* Events contain the useful error. */ } };
  const cancelDownload = async () => { await SenseVoiceAPI.cancelDownload(); setDownload(null); await refresh(); };
  const deleteModel = async (model: SenseVoiceModelInfo) => { await SenseVoiceAPI.deleteModel(model.name); setDownload(null); await refresh(); };
  if (loading) return <div className="flex h-28 items-center justify-center"><Loader2 className="h-5 w-5 animate-spin" /></div>;
  if (models.length === 0) return <p className="text-sm text-red-600">SenseVoice model metadata is unavailable.</p>;

  return <div className="space-y-3">{models.map((model) => {
    const progress = download?.modelName === model.name ? download.progress : senseVoiceDownloadProgress(model.status);
    const available = model.status === 'Available';
    const selected = available && selectedModel === model.name;
    const failed = typeof model.status === 'object' && ('Error' in model.status || 'Corrupted' in model.status);
    const precision = model.name.endsWith('fp32') ? 'FP32 / Quality' : model.name.endsWith('fp16') ? 'FP16 / Balanced' : 'INT8 / Fast';
    return <div key={model.name} className={`border p-4 transition-colors ${selected ? 'border-blue-500 bg-blue-50' : 'border-gray-200 bg-white'} ${available ? 'cursor-pointer' : ''}`} onClick={() => selectModel(model)}>
      <div className="flex items-start justify-between gap-4"><div className="min-w-0"><div className="flex items-center gap-2"><span aria-hidden="true" className="shrink-0 text-2xl leading-none">🗣️</span><h3 className="text-sm font-semibold text-gray-900">SenseVoice Small {precision}</h3>{selected && <CheckCircle2 className="h-4 w-4 text-blue-600" />}</div><p className="ml-8 mt-1 text-sm text-gray-600">{model.description}</p><p className="ml-8 mt-2 text-xs text-gray-500">{model.size_mb} MiB · Apple Neural Engine on Apple Silicon</p></div>
        <div className="flex shrink-0 items-center gap-1">{available ? <><span className="mr-2 text-xs font-medium text-green-700">{preparing && selected ? <span className="flex items-center gap-1"><Loader2 className="h-3 w-3 animate-spin" />Preparing</span> : 'Ready'}</span><Button variant="ghost" size="icon" title="Open model folder" onClick={(event) => { event.stopPropagation(); SenseVoiceAPI.openModelsFolder(); }}><FolderOpen className="h-4 w-4" /></Button><Button variant="ghost" size="icon" title="Delete model" onClick={(event) => { event.stopPropagation(); deleteModel(model); }}><Trash2 className="h-4 w-4 text-red-600" /></Button></> : progress === null ? <Button size="sm" onClick={(event) => { event.stopPropagation(); startDownload(model); }}><Download className="mr-2 h-4 w-4" />{failed ? 'Download again' : 'Download'}</Button> : <Button variant="ghost" size="icon" title="Cancel download" onClick={(event) => { event.stopPropagation(); cancelDownload(); }}><X className="h-4 w-4" /></Button>}</div></div>
      {progress !== null && <div className="mt-4 border-t border-gray-200 pt-3"><div className="mb-2 flex items-center justify-between text-xs text-gray-600"><span>Downloading {download?.modelName === model.name ? download.downloaded_mb.toFixed(1) : '0.0'} / {download?.modelName === model.name ? download.total_mb.toFixed(1) : model.size_mb} MiB</span><span>{progress}%{download?.modelName === model.name && download.speed_mbps > 0 ? ` · ${download.speed_mbps.toFixed(1)} MiB/s` : ''}</span></div><div className="h-2 overflow-hidden bg-gray-200"><div className="h-full bg-blue-600 transition-[width]" style={{ width: `${progress}%` }} /></div></div>}
      {available && <div className="mt-4 border-t border-gray-200 pt-4" onClick={(event) => event.stopPropagation()}><LanguageSelection selectedLanguage="auto" onLanguageChange={setSelectedLanguage} provider="senseVoice" model={model.name} /></div>}
    </div>;
  })}</div>;
}
