'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Search } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '@/components/ui/fluid-select';

interface EmbedderStatus {
  model: string;
  dim: number;
  model_present: boolean;
  loaded: boolean;
  available_models: Array<{
    id: string;
    name: string;
    dim: number;
    download_mb: number;
    present: boolean;
  }>;
}

interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  percent: number;
}

function modelDownloadSize(downloadMb: number): string {
  return downloadMb >= 1000 ? `${(downloadMb / 1000).toFixed(1)} GB` : `${downloadMb} MB`;
}

export function EmbeddingModelSettings() {
  const t = useT();
  const [status, setStatus] = useState<EmbedderStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<EmbedderStatus>('embedder_status')
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    refresh();
    let unProgress: (() => void) | undefined;
    let unReady: (() => void) | undefined;

    void (async () => {
      unProgress = await listen<DownloadProgress>('embedder-download-progress', (event) => {
        setProgress(event.payload);
      });
      unReady = await listen('embedder-ready', () => {
        setDownloading(false);
        setProgress(null);
        refresh();
      });
    })();

    return () => {
      unProgress?.();
      unReady?.();
    };
  }, [refresh]);

  const availableModels = useMemo(
    () => status?.available_models ?? [
      {
        id: 'multilingual-e5-small',
        name: 'Multilingual E5 Small',
        dim: 384,
        download_mb: 470,
        present: false,
      },
      { id: 'frida', name: 'FRIDA', dim: 1536, download_mb: 3300, present: false },
    ],
    [status?.available_models],
  );

  const changeModel = useCallback(async (modelId: string) => {
    if (selecting || downloading || modelId === status?.model) return;

    const model = availableModels.find((candidate) => candidate.id === modelId);
    if (!model) return;

    setError(null);
    setProgress(null);

    try {
      if (model.present) {
        setSelecting(true);
        await invoke('embedder_select_model', { model: modelId });
      } else {
        setDownloading(true);
        await invoke('embedder_download_model', { model: modelId });
      }
      refresh();
    } catch (cause) {
      setError(
        typeof cause === 'string'
          ? cause
          : model.present
            ? t('Could not switch the embedding model.')
            : t('Download failed.'),
      );
      setDownloading(false);
    } finally {
      setSelecting(false);
    }
  }, [availableModels, downloading, refresh, selecting, status?.model, t]);

  const caption = error
    ? error
    : downloading
      ? `${t('Downloading')} · ${progress?.percent ?? 0}%`
      : selecting
        ? t('Switching model…')
        : t('Searches meetings by meaning and works on this device.');

  return (
    <section className="settings-section settings-cell">
      <div className="settings-cell__row">
        <div className="settings-cell__avatar" aria-hidden="true">
          <Search size={20} />
        </div>

        <div className="settings-cell__text">
          <h3 className="settings-cell__label">{t('Search model')}</h3>
          <p className={`settings-cell__caption${error ? ' text-destructive' : ''}`}>
            {caption}
          </p>
        </div>

        <div className="settings-cell__control">
          <Select
            shape="rounded"
            value={status?.model ?? 'multilingual-e5-small'}
            onValueChange={changeModel}
            disabled={!status || selecting || downloading}
          >
            <SelectTrigger
              className="settings-cell__select settings-cell__device-select"
              aria-label={t('Search model')}
            />
            <SelectContent>
              {availableModels.map((model, index) => (
                <SelectItem key={model.id} value={model.id} index={index}>
                  {`${model.name} · ~${modelDownloadSize(model.download_mb)}`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
    </section>
  );
}
