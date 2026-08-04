'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Download, CheckCircle2, Loader2, AlertTriangle } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/fluid-button';
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/fluid-select';
import { Badge } from '@/components/ui/fluid-badge';
import { createMaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react';

interface VariantInfo {
  id: string;
  label: string;
  size_mb: number;
  present: boolean;
  /** The encoder runs on the Apple Neural Engine (Apple Silicon builds only). */
  neural_engine: boolean;
}

interface GigaamStatus {
  selected: string;
  model_present: boolean;
  loaded: boolean;
  loaded_variant: string | null;
  downloading: boolean;
  download_progress: DownloadProgress | null;
  variants: VariantInfo[];
  neural_engine_supported: boolean;
}

interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  percent: number;
  /**
   * `downloading` has byte progress; `extracting` and `compiling` are local CPU steps of the
   * Neural Engine model install and report none, so they render as indeterminate.
   */
  stage: 'downloading' | 'extracting' | 'compiling';
}

const IconTranscribe = createMaterialSymbol('graphic_eq', 'IconTranscribe');

function mb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

/**
 * GigaAM v3 (Russian ASR) model manager: pick a variant (CTC/RNN-T × int8/fp32) for A/B
 * quality testing, download it, and see load status. The transcript provider is set to
 * `gigaam` by the parent when this is shown.
 */
export function GigaamModelManager({ compact = false }: { compact?: boolean }) {
  const [status, setStatus] = useState<GigaamStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const t = useT();

  const refresh = useCallback(() => {
    invoke<GigaamStatus>('gigaam_status')
      .then((next) => {
        setStatus(next);
        setDownloading(next.downloading);
        setProgress(next.download_progress);
      })
      .catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unReady: (() => void) | undefined;
    let unError: (() => void) | undefined;
    let disposed = false;
    (async () => {
      const progressListener = await listen<DownloadProgress>('gigaam-download-progress', (e) => {
        setDownloading(true);
        setProgress(e.payload);
      });
      if (disposed) {
        progressListener();
        return;
      }
      unProgress = progressListener;

      const readyListener = await listen('gigaam-ready', () => {
        setDownloading(false);
        setSwitching(false);
        setProgress(null);
        refresh();
      });
      if (disposed) {
        readyListener();
        return;
      }
      unReady = readyListener;

      const errorListener = await listen<string>('gigaam-download-error', (e) => {
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
      // Read the backend state only after listeners are attached so completion cannot
      // be lost while this Settings tab is mounting.
      refresh();
    })();
    return () => {
      disposed = true;
      unProgress?.();
      unReady?.();
      unError?.();
    };
  }, [refresh]);

  const selectVariant = useCallback(
    async (variant: string) => {
      setError(null);
      setSwitching(true);
      try {
        await invoke('gigaam_select_variant', { variant });
        // Keep the persisted transcript config's model label in sync with the variant
        // that will actually transcribe (this component only renders when the GigaAM
        // provider is active).
        invoke('api_save_transcript_config', {
          provider: 'gigaam',
          model: `gigaam-v3-${variant}`,
          apiKey: null,
        }).catch((e) => console.error('Failed to sync transcript config:', e));
        refresh();
      } catch (e) {
        setError(typeof e === 'string' ? e : t('Failed to switch variant.'));
      } finally {
        setSwitching(false);
      }
    },
    [refresh],
  );

  const download = useCallback(async () => {
    setError(null);
    setDownloading(true);
    setProgress(null);
    try {
      await invoke('gigaam_download_model');
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : t('Download failed.'));
      setDownloading(false);
    }
  }, [refresh]);

  const selected = status?.variants.find((v) => v.id === status.selected) ?? null;
  const present = !!status?.model_present;
  const loaded = !!status?.loaded;
  // Unpacking and compiling the CoreML encoder have no byte counts to report.
  const indeterminate = !!progress && progress.stage !== 'downloading';
  const stageLabel = (stage: DownloadProgress['stage']) =>
    stage === 'extracting'
      ? t('Unpacking the Neural Engine model')
      : stage === 'compiling'
        ? t('Compiling the model for this Mac (one time)')
        : t('Downloading');
  // The selected variant is the one actually running only if it's loaded AND matches.
  const selectedActive = loaded && status?.loaded_variant === status?.selected;

  if (compact) {
    return (
      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconTranscribe size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Transcription engine')}</h3>
            <p className="settings-cell__caption">
              {downloading
                ? `${t('Downloading')} ${progress?.percent ?? 0}%`
                : t('Meetings are transcribed on this device.')}
            </p>
          </div>
          {present ? (
            <div className="settings-cell__control">
              <Select
                value={status?.selected ?? 'e2e-rnnt-fp32'}
                onValueChange={selectVariant}
                disabled={!status || downloading || switching}
              >
                <SelectTrigger className="settings-cell__select settings-cell__device-select" placeholder={t('Model variant')} />
                <SelectContent>
                  {(status?.variants ?? []).map((variant, index) => {
                    const unsupported = variant.neural_engine && !status?.neural_engine_supported;
                    return (
                      <SelectItem key={variant.id} index={index} value={variant.id} disabled={unsupported}>
                        {variant.label}
                      </SelectItem>
                    );
                  })}
                </SelectContent>
              </Select>
            </div>
          ) : (
            <Button
              variant="tertiary"
              onClick={download}
              disabled={!status || downloading}
              className="shrink-0 rounded-full"
            >
              {downloading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
              {t('Download variant (~')}{selected?.size_mb ?? '?'}{t(' MB)')}
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
          <h3 className="text-sm font-semibold text-foreground">{t('GigaAM v3 (Russian)')}</h3>
          <p className="text-xs text-muted-foreground">
            {t('Sber ASR · punctuated & capitalized · fully local')}
          </p>
        </div>
        {selectedActive ? (
          <Badge color="green" size="sm" className="gap-1">
            <CheckCircle2 className="h-3.5 w-3.5" /> {t('Active')}
          </Badge>
        ) : present ? (
          <Badge color="gray" size="sm">{t('Installed — restart to load')}</Badge>
        ) : (
          <Badge color="gray" size="sm">{t('Not downloaded')}</Badge>
        )}
      </div>

      {/* Variant selector for A/B quality testing */}
      <div className="mt-4">
        <label className="mb-1 block text-xs font-medium text-muted-foreground">{t('Model variant')}</label>
        <Select
          value={status?.selected ?? 'e2e-rnnt-fp32'}
          onValueChange={selectVariant}
          disabled={!status || downloading || switching}
        >
          <SelectTrigger className="w-full" placeholder={t('Model variant')} />
          <SelectContent>
          {(status?.variants ?? []).map((v, index) => {
            // An Apple Silicon Mac on macOS 13 lists the Neural Engine model but cannot load
            // it, so the option stays visible (and explains itself) instead of being selectable.
            const unsupported = v.neural_engine && !status?.neural_engine_supported;
            return (
              <SelectItem key={v.id} index={index} value={v.id} disabled={unsupported}>
                {v.label} · ~{v.size_mb}{t(' MB')}
                {unsupported ? t(' · needs macOS 14') : v.present ? t(' · installed') : ''}
              </SelectItem>
            );
          })}
          </SelectContent>
        </Select>
        <p className="mt-1 text-xs text-muted-foreground">
          {selected?.neural_engine
            ? t('The encoder runs on the Apple Neural Engine (macOS 14+): the same transcripts as fp32, several times faster and much lighter on the CPU. The first load takes about a minute while macOS optimizes the model for this Mac.')
            : t('RNN-T usually beats CTC on accuracy; fp32 avoids int8 quantization loss (larger & slower).')}
        </p>
      </div>

      <div className="mt-4">
        {downloading ? (
          <div>
            <div className="mb-1.5 flex items-center justify-between text-xs text-muted-foreground">
              <span className="flex items-center gap-1.5">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {progress ? `${stageLabel(progress.stage)}${indeterminate ? '…' : ` ${progress.file}`}` : `${t('Downloading')}…`}
              </span>
              {progress && progress.total > 0 && (
                <span>
                  {mb(progress.downloaded)} / {mb(progress.total)} · {progress.percent}%
                </span>
              )}
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={`h-full rounded-full bg-primary transition-all${indeterminate ? ' animate-pulse' : ''}`}
                style={{ width: indeterminate ? '100%' : `${progress?.percent ?? 0}%` }}
              />
            </div>
          </div>
        ) : switching ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('Loading')} {selected?.label ?? t('variant')}…
          </div>
        ) : selectedActive ? (
          <div className="flex items-center gap-2 text-sm font-medium text-success">
            <CheckCircle2 className="h-4 w-4" />
            {t('Ready — recordings will transcribe with this variant')}
          </div>
        ) : (
          <Button variant="tertiary"
            onClick={download}
            className="flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Download className="h-4 w-4" />
            {present ? t('Re-download variant') : `${t('Download variant (~')}${selected?.size_mb ?? '?'}${t(' MB)')}`}
          </Button>
        )}

        {/* Loaded-vs-selected mismatch hint (e.g. selected a new variant that still needs a download) */}
        {!switching && !downloading && loaded && !selectedActive && (
          <p className="mt-2 text-xs text-primary">
            {t('Currently running')} <span className="font-medium">{status?.loaded_variant}</span>.
            {present ? t(' Restart to load the selected variant.') : t(' Download the selected variant to switch.')}
          </p>
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
