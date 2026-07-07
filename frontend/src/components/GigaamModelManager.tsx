'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Download, CheckCircle2, Loader2, AlertTriangle } from 'lucide-react';

interface GigaamStatus {
  model: string;
  model_present: boolean;
  loaded: boolean;
}

interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  percent: number;
}

function mb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

/**
 * GigaAM v3 (Russian ASR) model manager: download + status. Mirrors the embedding-model
 * card. The transcript provider is set to `gigaam` by the parent when this is shown.
 */
export function GigaamModelManager() {
  const [status, setStatus] = useState<GigaamStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<GigaamStatus>('gigaam_status').then(setStatus).catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    refresh();
    let unProgress: (() => void) | undefined;
    let unReady: (() => void) | undefined;
    (async () => {
      unProgress = await listen<DownloadProgress>('gigaam-download-progress', (e) => setProgress(e.payload));
      unReady = await listen('gigaam-ready', () => {
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
      await invoke('gigaam_download_model');
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Download failed.');
      setDownloading(false);
    }
  }, [refresh]);

  const loaded = !!status?.loaded;
  const present = !!status?.model_present;

  return (
    <div className="rounded-xl border border-gray-200 p-5">
      <div className="mb-1 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-gray-900">GigaAM v3 (Russian)</h3>
          <p className="text-xs text-gray-400">
            Sber ASR · e2e-CTC · punctuated &amp; capitalized · ~224&nbsp;MB · fully local
          </p>
        </div>
        {loaded ? (
          <span className="flex items-center gap-1 rounded-full bg-green-50 px-2 py-0.5 text-xs font-medium text-green-700">
            <CheckCircle2 className="h-3.5 w-3.5" /> Active
          </span>
        ) : present ? (
          <span className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-500">Installed — restart to load</span>
        ) : (
          <span className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-500">Not downloaded</span>
        )}
      </div>

      <div className="mt-4">
        {downloading ? (
          <div>
            <div className="mb-1.5 flex items-center justify-between text-xs text-gray-500">
              <span className="flex items-center gap-1.5">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                Downloading{progress ? ` ${progress.file}` : '…'}
              </span>
              {progress && progress.total > 0 && (
                <span>
                  {mb(progress.downloaded)} / {mb(progress.total)} · {progress.percent}%
                </span>
              )}
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-gray-100">
              <div
                className="h-full rounded-full bg-blue-500 transition-all"
                style={{ width: `${progress?.percent ?? 0}%` }}
              />
            </div>
          </div>
        ) : loaded ? (
          <div className="flex items-center gap-2 text-sm font-medium text-green-600">
            <CheckCircle2 className="h-4 w-4" />
            Ready — recordings will transcribe with GigaAM
          </div>
        ) : (
          <button
            onClick={download}
            className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700"
          >
            <Download className="h-4 w-4" />
            {present ? 'Re-download model' : 'Download model'}
          </button>
        )}

        {error && (
          <div className="mt-3 flex items-start gap-1.5 text-sm text-red-600">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
