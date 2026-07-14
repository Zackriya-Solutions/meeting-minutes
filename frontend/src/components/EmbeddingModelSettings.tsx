'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Search, Download, CheckCircle2, Loader2, AlertTriangle, RotateCw } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

// Mirrors the Rust `embedder_status` payload (pipeline::commands).
interface EmbedderStatus {
  model: string;
  dim: number;
  model_present: boolean;
  loaded: boolean;
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

export function EmbeddingModelSettings() {
  const t = useT();
  const [status, setStatus] = useState<EmbedderStatus | null>(null);
  const [indexStatus, setIndexStatus] = useState<IndexingStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [repairing, setRepairing] = useState(false);
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

  const download = useCallback(async () => {
    setError(null);
    setDownloading(true);
    setProgress(null);
    try {
      await invoke('embedder_download_model');
      // Success is also signalled by the `embedder-ready` event; refresh defensively.
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Download failed.'));
      setDownloading(false);
    }
  }, [refresh, t]);

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
  const activeJobs = (indexStatus?.queued_jobs ?? 0) + (indexStatus?.running_jobs ?? 0);
  const semanticCoverage = indexStatus?.chunks_total
    ? Math.round((indexStatus.embeddings_done / indexStatus.chunks_total) * 100)
    : indexStatus?.indexable_meetings === 0
      ? 100
      : 0;

  return (
    <div className="mt-6 max-w-2xl">
      <div className="rounded-xl border border-[var(--border-subtle)] p-5">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--gold-soft)]">
            <Search className="h-5 w-5 text-[var(--gold)]" />
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Semantic search model')}</h3>
            <p className="mt-1 text-sm leading-relaxed text-[var(--fg2)]">
              {t('Powers meaning-based (vector) search and Chat with archive. Runs fully locally —')}{' '}
              <span className="font-medium text-[var(--fg2)]">{modelName}</span> ({dim}
              {t('-dim), ~470 MB. Until it\'s installed, Search and Chat use keyword (FTS) matching only.')}
            </p>

            <div className="mt-4">
              {downloading ? (
                <div>
                  <div className="mb-1.5 flex items-center justify-between text-xs text-[var(--fg2)]">
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
                  {t('Active — semantic search enabled')}
                </div>
              ) : present ? (
                <div className="flex flex-wrap items-center gap-3">
                  <span className="flex items-center gap-2 text-sm text-[var(--fg2)]">
                    <CheckCircle2 className="h-4 w-4 text-[var(--fg3)]" />
                    {t('Installed — restart the app to activate')}
                  </span>
                  <button
                    onClick={download}
                    className="flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] px-3 py-1.5 text-xs text-[var(--fg2)] hover:bg-[var(--bg-elevated)]"
                  >
                    <RotateCw className="h-3.5 w-3.5" />
                    {t('Re-download')}
                  </button>
                </div>
              ) : (
                <button
                  onClick={download}
                  className="flex items-center gap-2 rounded-lg bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)]"
                >
                  <Download className="h-4 w-4" />
                  {t('Download model')}
                </button>
              )}

              {error && (
                <div className="mt-3 flex items-start gap-1.5 text-sm text-[var(--danger)]">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                  <span>{error}</span>
                </div>
              )}
            </div>

            <div className="mt-5 border-t border-[var(--border-subtle)] pt-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h4 className="text-sm font-semibold text-[var(--fg1)]">{t('Archive search index')}</h4>
                  {indexStatus ? (
                    <p className="mt-1 text-xs leading-relaxed text-[var(--fg3)]">
                      {indexStatus.chunked_meetings} {t('of')} {indexStatus.indexable_meetings}{' '}
                      {t('meetings searchable')} · {indexStatus.embeddings_done} {t('of')}{' '}
                      {indexStatus.chunks_total} {t('semantic chunks ready')}
                    </p>
                  ) : (
                    <p className="mt-1 text-xs text-[var(--fg3)]">{t('Index status unavailable')}</p>
                  )}
                </div>
                <button
                  onClick={repairIndex}
                  disabled={repairing || activeJobs > 0}
                  className="flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] px-3 py-1.5 text-xs text-[var(--fg2)] hover:bg-[var(--bg-elevated)] disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <RotateCw className={`h-3.5 w-3.5 ${repairing || activeJobs > 0 ? 'animate-spin' : ''}`} />
                  {repairing || activeJobs > 0 ? t('Repairing index…') : t('Check and repair index')}
                </button>
              </div>

              {indexStatus && (
                <>
                  <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-[var(--bg-elevated)]">
                    <div
                      className="h-full rounded-full bg-[var(--gold)] transition-all"
                      style={{ width: `${loaded ? semanticCoverage : 0}%` }}
                    />
                  </div>
                  <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-[var(--fg3)]">
                    {loaded ? (
                      <span>{semanticCoverage}% {t('semantic coverage')}</span>
                    ) : (
                      <span>{t('Install the model to build semantic coverage')}</span>
                    )}
                    {indexStatus.embeddings_pending > 0 && (
                      <span>{indexStatus.embeddings_pending} {t('pending')}</span>
                    )}
                    {indexStatus.embeddings_failed > 0 && (
                      <span className="text-[var(--danger)]">
                        {indexStatus.embeddings_failed} {t('failed')}
                      </span>
                    )}
                    {activeJobs > 0 && <span>{activeJobs} {t('background jobs active')}</span>}
                    {indexStatus.unresolved_failed_jobs > 0 && (
                      <span className="text-[var(--danger)]">
                        {indexStatus.unresolved_failed_jobs} {t('jobs need retry')}
                      </span>
                    )}
                    {!indexStatus.needs_repair && activeJobs === 0 && (
                      <span className="text-[var(--success)]">{t('Index is healthy')}</span>
                    )}
                  </div>
                </>
              )}
            </div>
          </div>
        </div>
      </div>

      <p className="mt-3 px-1 text-xs text-[var(--fg3)]">
        {t('Embeddings and search stay on-device. Only summaries, extraction, and chat prompts are sent to your configured LLM provider (GigaChat / DeepSeek).')}
      </p>
    </div>
  );
}
