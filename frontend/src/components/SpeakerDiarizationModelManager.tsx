import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Download, Loader2, Trash2, Users } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from './ui/button';

interface SpeakerModelStatus {
  id: string;
  status: 'available' | 'missing' | 'corrupt';
  size_mb: number;
  path: string;
  error?: string | null;
}

interface DownloadProgress {
  model_id: string;
  progress: number;
  downloaded_mb: number;
  total_mb: number;
  status: string;
}

export function SpeakerDiarizationModelManager() {
  const [status, setStatus] = useState<SpeakerModelStatus | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const next = await invoke<SpeakerModelStatus>('speaker_diarization_get_status');
    setStatus(next);
  }, []);

  useEffect(() => {
    refresh().catch(error => console.warn('Failed to read speaker model status:', error));
  }, [refresh]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    const setup = async () => {
      unlisteners.push(await listen<DownloadProgress>(
        'speaker-diarization-model-download-progress',
        ({ payload }) => setProgress(payload.progress)
      ));
      unlisteners.push(await listen('speaker-diarization-model-download-complete', async () => {
        setBusy(false);
        setProgress(null);
        await refresh();
        toast.success('Speaker diarization is ready');
      }));
      unlisteners.push(await listen<{ error: string }>(
        'speaker-diarization-model-download-error',
        ({ payload }) => {
          setBusy(false);
          setProgress(null);
          toast.error('Speaker model download failed', { description: payload.error });
        }
      ));
    };
    setup().catch(error => console.warn('Failed to listen for speaker model events:', error));
    return () => unlisteners.forEach(unlisten => unlisten());
  }, [refresh]);

  const download = async () => {
    setBusy(true);
    setProgress(0);
    try {
      await invoke('speaker_diarization_download_model');
    } catch (error) {
      setBusy(false);
      setProgress(null);
      toast.error('Speaker model download failed', { description: String(error) });
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await invoke('speaker_diarization_delete_model');
      await refresh();
      toast.success('Speaker diarization model removed');
    } catch (error) {
      toast.error('Failed to remove speaker model', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const available = status?.status === 'available';
  return (
    <div className="rounded-lg border p-3 space-y-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex gap-2">
          <Users className="h-4 w-4 mt-0.5 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium">Speaker Diarization</div>
            <p className="text-xs text-muted-foreground">
              Pyannote segmentation + 3D-Speaker ERes2Net · about {Math.round(status?.size_mb ?? 44)} MB
            </p>
          </div>
        </div>
        <span className={`text-xs font-medium ${available ? 'text-emerald-600' : 'text-muted-foreground'}`}>
          {available ? 'Installed' : status?.status === 'corrupt' ? 'Needs repair' : 'Not installed'}
        </span>
      </div>

      {progress !== null && (
        <div className="space-y-1">
          <div className="h-2 rounded-full bg-gray-200 overflow-hidden">
            <div className="h-full bg-blue-600 transition-all" style={{ width: `${progress}%` }} />
          </div>
          <div className="text-xs text-muted-foreground">Downloading… {progress}%</div>
        </div>
      )}

      <div className="flex justify-end gap-2">
        {available ? (
          <Button variant="outline" size="sm" onClick={remove} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 mr-2 animate-spin" /> : <Trash2 className="h-4 w-4 mr-2" />}
            Delete
          </Button>
        ) : (
          <Button size="sm" onClick={download} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 mr-2 animate-spin" /> : <Download className="h-4 w-4 mr-2" />}
            {status?.status === 'corrupt' ? 'Repair' : 'Download'}
          </Button>
        )}
      </div>
    </div>
  );
}
