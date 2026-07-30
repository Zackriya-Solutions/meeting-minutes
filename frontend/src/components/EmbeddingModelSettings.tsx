'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Search, Download, CheckCircle2, Loader2, AlertTriangle, RotateCw } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';

// Mirrors the Rust `embedder_status` payload (pipeline::commands).
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

interface IndexingStatus {
  indexable_meetings: number;
  chunked_meetings: number;
  chunks_total: number;
  embeddings_done: number;
  embeddings_pending: number;
  embeddings_failed: number;
  queued_jobs: number;
  running_jobs: number;
  unresolved_failed_jobs: number;
  needs_repair: boolean;
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

function modelDownloadSize(downloadMb: number): string {
  return downloadMb >= 1000 ? `${(downloadMb / 1000).toFixed(1)} GB` : `${downloadMb} MB`;
}

export function EmbeddingModelSettings() {
  const t = useT();
  const [status, setStatus] = useState<EmbedderStatus | null>(null);
  const [indexStatus, setIndexStatus] = useState<IndexingStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<EmbedderStatus>('embedder_status')
      .then(setStatus)
      .catch(() => setStatus(null));
    invoke<IndexingStatus>('indexing_status')
      .then(setIndexStatus)
      .catch(() => setIndexStatus(null));
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

  // Indexing runs in the persistent background queue. Poll only while Settings
  // is open; this also reflects jobs resumed after an app restart.
  useEffect(() => {
    const timer = window.setInterval(refresh, 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (indexStatus && indexStatus.queued_jobs + indexStatus.running_jobs === 0) {
      setRepairing(false);
    }
  }, [indexStatus]);

  const download = useCallback(async (model = status?.model ?? 'multilingual-e5-small') => {
    setError(null);
    setDownloading(true);
    setProgress(null);
    try {
      await invoke('embedder_download_model', { model });
      // Success is also signalled by the `embedder-ready` event; refresh defensively.
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Download failed.'));
      setDownloading(false);
    }
  }, [refresh, status?.model, t]);

  const selectModel = useCallback(async (model: string) => {
    if (model === status?.model || selecting || downloading) return;
    setError(null);
    setSelecting(true);
    try {
      await invoke('embedder_select_model', { model });
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Could not switch the embedding model.'));
    } finally {
      setSelecting(false);
    }
  }, [downloading, refresh, selecting, status?.model, t]);

  const repairIndex = useCallback(async () => {
    setError(null);
    setRepairing(true);
    try {
      await invoke('run_backfill');
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Could not start index repair.'));
      setRepairing(false);
    }
  }, [refresh, t]);

  const loaded = !!status?.loaded;
  const present = !!status?.model_present;
  const modelName = status?.model ?? 'multilingual-e5-small';
  const dim = status?.dim ?? 384;
  const availableModels = status?.available_models ?? [
    { id: 'multilingual-e5-small', name: 'Multilingual E5 Small', dim: 384, download_mb: 470, present },
    { id: 'frida', name: 'FRIDA', dim: 1536, download_mb: 3300, present: false },
  ];
  const selectedDownloadMb =
    availableModels.find((model) => model.id === modelName)?.download_mb ?? 470;
  const activeJobs = (indexStatus?.queued_jobs ?? 0) + (indexStatus?.running_jobs ?? 0);
  const semanticCoverage = indexStatus?.chunks_total
    ? Math.round((indexStatus.embeddings_done / indexStatus.chunks_total) * 100)
    : indexStatus?.indexable_meetings === 0
      ? 100
      : 0;

  return (
    <div>
      <section className="settings-section">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10">
            <Search className="h-5 w-5 text-primary" />
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-foreground">{t('Semantic search model')}</h3>
            <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
              {t('Powers meaning-based (vector) search and Chat with archive. Runs fully locally —')}{' '}
              <span className="font-medium text-muted-foreground">{modelName}</span> ({dim}{' '}
              {t('dimensions')}, ~{modelDownloadSize(selectedDownloadMb)}).{' '}
              {t('Until it is installed, Search and Chat use keyword (FTS) matching only.')}
            </p>

            <div className="mt-4 grid gap-2 sm:grid-cols-2">
              {availableModels.map((model) => {
                const selected = model.id === modelName;
                return (
                  <button
                    key={model.id}
                    type="button"
                    disabled={selecting || downloading}
                    onClick={() => model.present ? selectModel(model.id) : download(model.id)}
                    className={`rounded-xl border p-3 text-left transition-colors ${
                      selected
                        ? 'border-primary/40 bg-primary/10'
                        : 'border-border hover:bg-muted'
                    }`}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-semibold text-foreground">{model.name}</span>
                      {selected && <CheckCircle2 className="h-4 w-4 text-primary" />}
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {model.dim} {t('dimensions')} · ~{modelDownloadSize(model.download_mb)}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {model.id === 'frida'
                        ? t('Russian-first retrieval model. Uses search_query/search_document prefixes and CLS pooling.')
                        : t('Compact multilingual retrieval model. Recommended for most devices.')}
                    </p>
                  </button>
                );
              })}
            </div>

            {modelName === 'frida' && (
              <div className="mt-3 rounded-lg border border-primary/40 bg-primary/10 p-3 text-xs leading-relaxed text-primary">
                {t('FRIDA is optional and large (~3.3 GB ONNX). Memento downloads a community ONNX conversion of the ai-forever/FRIDA weights. Switching models rebuilds the semantic index in the background; keyword search remains available during reindexing.')}
              </div>
            )}

            <div className="mt-4">
              {downloading ? (
                <div>
                  <div className="mb-1.5 flex items-center justify-between text-xs text-muted-foreground">
                    <span className="flex items-center gap-1.5">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t('Downloading')}{progress ? ` ${progress.file}` : '…'}
                    </span>
                    {progress && progress.total > 0 && (
                      <span>
                        {mb(progress.downloaded)} / {mb(progress.total)} · {progress.percent}%
                      </span>
                    )}
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className="h-full rounded-full bg-primary transition-all"
                      style={{ width: `${progress?.percent ?? 0}%` }}
                    />
                  </div>
                </div>
              ) : loaded ? (
                <div className="flex items-center gap-2 text-sm font-medium text-success">
                  <CheckCircle2 className="h-4 w-4" />
                  {t('Active — semantic search enabled')}
                </div>
              ) : present ? (
                <div className="flex flex-wrap items-center gap-3">
                  <span className="flex items-center gap-2 text-sm text-muted-foreground">
                    <CheckCircle2 className="h-4 w-4 text-muted-foreground" />
                    {t('Installed — restart the app to activate')}
                  </span>
                  <button
                    onClick={() => download(modelName)}
                    className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground hover:bg-muted"
                  >
                    <RotateCw className="h-3.5 w-3.5" />
                    {t('Re-download')}
                  </button>
                </div>
              ) : (
                <button
                  onClick={() => download(modelName)}
                  className="flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
                >
                  <Download className="h-4 w-4" />
                  {t('Download model')}
                </button>
              )}

              {error && (
                <div className="mt-3 flex items-start gap-1.5 text-sm text-destructive">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                  <span>{error}</span>
                </div>
              )}
            </div>

            <div className="mt-5 border-t border-border pt-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h4 className="text-sm font-semibold text-foreground">{t('Archive search index')}</h4>
                  {indexStatus ? (
                    <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                      {indexStatus.chunked_meetings} {t('of')} {indexStatus.indexable_meetings}{' '}
                      {t('meetings searchable')} · {indexStatus.embeddings_done} {t('of')}{' '}
                      {indexStatus.chunks_total} {t('semantic chunks ready')}
                    </p>
                  ) : (
                    <p className="mt-1 text-xs text-muted-foreground">{t('Index status unavailable')}</p>
                  )}
                </div>
                <button
                  onClick={repairIndex}
                  disabled={repairing || activeJobs > 0}
                  className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <RotateCw className={`h-3.5 w-3.5 ${repairing || activeJobs > 0 ? 'animate-spin' : ''}`} />
                  {repairing || activeJobs > 0 ? t('Repairing index…') : t('Check and repair index')}
                </button>
              </div>

              {indexStatus && (
                <>
                  <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className="h-full rounded-full bg-primary transition-all"
                      style={{ width: `${loaded ? semanticCoverage : 0}%` }}
                    />
                  </div>
                  <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                    {loaded ? (
                      <span>{semanticCoverage}% {t('semantic coverage')}</span>
                    ) : (
                      <span>{t('Install the model to build semantic coverage')}</span>
                    )}
                    {indexStatus.embeddings_pending > 0 && (
                      <span>{indexStatus.embeddings_pending} {t('pending')}</span>
                    )}
                    {indexStatus.embeddings_failed > 0 && (
                      <span className="text-destructive">
                        {indexStatus.embeddings_failed} {t('failed')}
                      </span>
                    )}
                    {activeJobs > 0 && <span>{activeJobs} {t('background jobs active')}</span>}
                    {indexStatus.unresolved_failed_jobs > 0 && (
                      <span className="text-destructive">
                        {indexStatus.unresolved_failed_jobs} {t('jobs need retry')}
                      </span>
                    )}
                    {!indexStatus.needs_repair && activeJobs === 0 && (
                      <span className="text-success">{t('Index is healthy')}</span>
                    )}
                  </div>
                </>
              )}
            </div>
          </div>
        </div>
      </section>

      <p className="mt-3 px-1 text-xs text-muted-foreground">
        {t('Embeddings and search stay on-device. Only summaries, extraction, and chat prompts are sent to your configured LLM provider (GigaChat / DeepSeek).')}
      </p>
    </div>
  );
}
