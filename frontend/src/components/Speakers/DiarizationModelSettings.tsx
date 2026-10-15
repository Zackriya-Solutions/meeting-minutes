'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { DiarizationModelsStatus } from '@/types';

export const formatMegabytes = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;

export function useDiarizationModels() {
  const [status, setStatus] = useState<DiarizationModelsStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<DiarizationModelsStatus>('diarization_models_status'));
    } catch (error) {
      console.error('Failed to read speaker model status:', error);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen<{ percent: number }>('diarization-model-download-progress', ({ payload }) => {
      setDownloading(payload.percent < 100);
      setProgress(payload.percent);
      if (payload.percent >= 100) void refresh();
    });
    return () => { unlisten.then((u) => u()); };
  }, [refresh]);

  const download = useCallback(async () => {
    setDownloading(true);
    setProgress(0);
    try {
      setStatus(await invoke<DiarizationModelsStatus>('diarization_download_models'));
    } finally {
      setDownloading(false);
    }
  }, []);

  const remove = useCallback(async () => {
    try {
      setStatus(await invoke<DiarizationModelsStatus>('diarization_delete_models'));
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'Failed to delete speaker models');
    }
  }, []);

  return { status, downloading, progress, refresh, download, remove };
}

export function DiarizationModelSettings() {
  const { status, downloading, progress, download, remove } = useDiarizationModels();
  if (!status) return null;
  return (
    <div className="flex items-center justify-between gap-3 text-sm text-gray-600">
      <div className="flex-1">
        {downloading ? (
          <div className="space-y-1">
            <div>Downloading speaker models… {progress}%</div>
            <Progress value={progress} />
          </div>
        ) : status.installed ? (
          `Speaker models downloaded (${formatMegabytes(status.total_bytes)})`
        ) : (
          `Speaker models not downloaded (${formatMegabytes(status.total_bytes)}); they download on first use`
        )}
      </div>
      {!downloading && (status.installed
        ? <Button size="sm" variant="outline" onClick={() => void remove()}>Delete</Button>
        : <Button size="sm" variant="outline" onClick={() => void download().catch((e) => toast.error(typeof e === 'string' ? e : 'Failed to download speaker models'))}>Download</Button>)}
    </div>
  );
}
