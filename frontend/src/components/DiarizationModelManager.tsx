'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Download, CheckCircle2, Loader2, AlertTriangle } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/fluid-button';
import { createMaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react';

interface DiarizationStatus {
  available: boolean;
  model_dir: string;
  segmentation_present: boolean;
  embedding_present: boolean;
  /** Combined download size, reported by the backend so this card can't drift from it. */
  download_mb: number;
  downloading: boolean;
}

interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  percent: number;
}

const IconSpeakers = createMaterialSymbol('groups', 'IconSpeakers');

/**
 * Speaker-recognition (diarization) model manager.
 *
 * These two models used to have no home in Settings at all: the only way to get them was to
 * open a saved meeting, press "Detect speakers", and let it fail. Meanwhile the automatic
 * post-meeting pass needs them, so a fresh install silently produced unlabeled transcripts.
 */
export function DiarizationModelManager({ compact = false }: { compact?: boolean }) {
  const [status, setStatus] = useState<DiarizationStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const t = useT();

  const refresh = useCallback(() => {
    invoke<DiarizationStatus>('diarization_status')
      .then((next) => {
        setStatus(next);
        setDownloading(next.downloading);
      })
      .catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unReady: (() => void) | undefined;
    let unError: (() => void) | undefined;
    let disposed = false;
    (async () => {
      const progressListener = await listen<DownloadProgress>(
        'diarization-download-progress',
        (e) => {
          setDownloading(true);
          setProgress(e.payload);
        },
      );
      if (disposed) {
        progressListener();
        return;
      }
      unProgress = progressListener;

      const readyListener = await listen('diarization-ready', () => {
        setDownloading(false);
        setProgress(null);
        setError(null);
        refresh();
      });
      if (disposed) {
        readyListener();
        return;
      }
      unReady = readyListener;

      const errorListener = await listen<string>('diarization-download-error', (e) => {
        setDownloading(false);
        setProgress(null);
        setError(e.payload);
        refresh();
      });
      if (disposed) {
        errorListener();
        return;
      }
      unError = errorListener;
      // Read state only once the listeners exist, so a download finishing while this tab
      // mounts cannot be missed.
      refresh();
    })();
    return () => {
      disposed = true;
      unProgress?.();
      unReady?.();
      unError?.();
    };
  }, [refresh]);

  const download = useCallback(async () => {
    setError(null);
    setDownloading(true);
    setProgress(null);
    try {
      await invoke('download_diarization_models');
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Download failed.'));
      setDownloading(false);
    }
  }, [refresh, t]);

  const present = !!status?.available;
  const sizeMb = status?.download_mb ?? 34;

  const caption = downloading
    ? `${t('Downloading')} ${progress?.percent ?? 0}%`
    : present
      ? t('Speakers are separated on this device.')
      : t('Without these models transcripts have no speaker labels.');

  if (compact) {
    return (
      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconSpeakers size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Speaker recognition')}</h3>
            <p className="settings-cell__caption">{caption}</p>
          </div>
          {present ? (
            <span className="settings-cell__control text-success">
              <CheckCircle2 className="h-4 w-4" />
            </span>
          ) : (
            <Button
              variant="tertiary"
              onClick={download}
              disabled={!status || downloading}
              className="shrink-0 rounded-full"
            >
              {downloading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Download className="h-4 w-4" />
              )}
              {t('Download (~')}{sizeMb}{t(' MB)')}
            </Button>
          )}
        </div>
        {error && <p className="px-4 pb-3 text-xs text-destructive">{error}</p>}
      </section>
    );
  }

  return (
    <div className="rounded-xl border border-border p-5">
      <div className="mb-1 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-foreground">{t('Speaker recognition')}</h3>
          <p className="text-xs text-muted-foreground">
            {t('Separates who said what · fully local')}
          </p>
        </div>
        {present ? (
          <span className="flex items-center gap-1 text-xs font-medium text-success">
            <CheckCircle2 className="h-3.5 w-3.5" /> {t('Installed')}
          </span>
        ) : (
          <span className="text-xs text-muted-foreground">{t('Not downloaded')}</span>
        )}
      </div>

      <p className="mt-2 text-xs text-muted-foreground">
        {t('Two models: voice activity segmentation and speaker embeddings. Meetings are labeled by speaker automatically after recording; without them the transcript stays unlabeled.')}
      </p>

      <div className="mt-4">
        {downloading ? (
          <div>
            <div className="mb-1.5 flex items-center justify-between text-xs text-muted-foreground">
              <span className="flex items-center gap-1.5">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {progress ? `${t('Downloading')} ${progress.file}` : `${t('Downloading')}…`}
              </span>
              {progress && progress.total > 0 && <span>{progress.percent}%</span>}
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary transition-all"
                style={{ width: `${progress?.percent ?? 0}%` }}
              />
            </div>
          </div>
        ) : present ? (
          <div className="flex items-center gap-2 text-sm font-medium text-success">
            <CheckCircle2 className="h-4 w-4" />
            {t('Ready — meetings will be split by speaker')}
          </div>
        ) : (
          <Button variant="tertiary" onClick={download} disabled={!status} className="flex items-center gap-2 rounded-lg">
            <Download className="h-4 w-4" />
            {t('Download (~')}{sizeMb}{t(' MB)')}
          </Button>
        )}

        {error && (
          <div className="mt-3 flex items-start gap-1.5 text-sm text-destructive">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
