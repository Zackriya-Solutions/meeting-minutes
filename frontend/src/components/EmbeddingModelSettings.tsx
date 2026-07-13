'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Search, Download, CheckCircle2, Loader2, AlertTriangle, RotateCw } from '@/components/memento/LucideCompat';

// Mirrors the Rust `embedder_status` payload (pipeline::commands).
interface EmbedderStatus {
  model: string;
  dim: number;
  model_present: boolean;
  loaded: boolean;
}

// Mirrors the Rust `embedder-download-progress` event payload.
interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  percent: number;
}

function mb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

export function EmbeddingModelSettings() {
  const [status, setStatus] = useState<EmbedderStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
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
    (async () => {
      unProgress = await listen<DownloadProgress>('embedder-download-progress', (e) => setProgress(e.payload));
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

  const download = useCallback(async () => {
    setError(null);
    setDownloading(true);
    setProgress(null);
    try {
      await invoke('embedder_download_model');
      // Success is also signalled by the `embedder-ready` event; refresh defensively.
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Не удалось загрузить модель.');
      setDownloading(false);
    }
  }, [refresh]);

  const loaded = !!status?.loaded;
  const present = !!status?.model_present;
  const modelName = status?.model ?? 'multilingual-e5-small';
  const dim = status?.dim ?? 384;

  return (
    <div className="mt-6 max-w-2xl">
      <div className="rounded-xl border border-[var(--border-subtle)] p-5">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--gold-soft)]">
            <Search className="h-5 w-5 text-[var(--gold)]" />
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-[var(--fg1)]">Модель смыслового поиска</h3>
            <p className="mt-1 text-sm leading-relaxed text-[var(--fg2)]">
              Нужна для смыслового поиска и чата с архивом. Работает локально —{' '}
              <span className="font-medium text-[var(--fg2)]">{modelName}</span> ({dim} измерений), ~470&nbsp;МБ.
              До установки поиск и чат используют только совпадения по словам.
            </p>

            <div className="mt-4">
              {downloading ? (
                <div>
                  <div className="mb-1.5 flex items-center justify-between text-xs text-[var(--fg2)]">
                    <span className="flex items-center gap-1.5">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      Загружаю{progress ? ` ${progress.file}` : '…'}
                    </span>
                    {progress && progress.total > 0 && (
                      <span>
                        {mb(progress.downloaded)} / {mb(progress.total)} · {progress.percent}%
                      </span>
                    )}
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-[var(--bg-elevated)]">
                    <div
                      className="h-full rounded-full bg-[var(--gold)] transition-all"
                      style={{ width: `${progress?.percent ?? 0}%` }}
                    />
                  </div>
                </div>
              ) : loaded ? (
                <div className="flex items-center gap-2 text-sm font-medium text-[var(--success)]">
                  <CheckCircle2 className="h-4 w-4" />
                  Активна — смысловой поиск включён
                </div>
              ) : present ? (
                <div className="flex flex-wrap items-center gap-3">
                  <span className="flex items-center gap-2 text-sm text-[var(--fg2)]">
                    <CheckCircle2 className="h-4 w-4 text-[var(--fg3)]" />
                    Установлена — перезапусти приложение для активации
                  </span>
                  <button
                    onClick={download}
                    className="flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] px-3 py-1.5 text-xs text-[var(--fg2)] hover:bg-[var(--bg-elevated)]"
                  >
                    <RotateCw className="h-3.5 w-3.5" />
                    Загрузить снова
                  </button>
                </div>
              ) : (
                <button
                  onClick={download}
                  className="flex items-center gap-2 rounded-lg bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)]"
                >
                  <Download className="h-4 w-4" />
                  Загрузить модель
                </button>
              )}

              {error && (
                <div className="mt-3 flex items-start gap-1.5 text-sm text-[var(--danger)]">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                  <span>{error}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      <p className="mt-3 px-1 text-xs text-[var(--fg3)]">
        Индексы и поиск остаются на устройстве. Настроенному провайдеру модели отправляются только запросы
        для сводок, извлечения данных и чата.
      </p>
    </div>
  );
}
