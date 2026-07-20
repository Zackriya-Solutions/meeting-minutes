'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Download, CheckCircle2, Loader2, AlertTriangle } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

interface VariantInfo {
  id: string;
  label: string;
  size_mb: number;
  present: boolean;
}

interface GigaamStatus {
  selected: string;
  model_present: boolean;
  loaded: boolean;
  loaded_variant: string | null;
  variants: VariantInfo[];
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
 * GigaAM v3 (Russian ASR) model manager: pick a variant (CTC/RNN-T × int8/fp32) for A/B
 * quality testing, download it, and see load status. The transcript provider is set to
 * `gigaam` by the parent when this is shown.
 */
export function GigaamModelManager() {
  const [status, setStatus] = useState<GigaamStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const t = useT();

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
        setSwitching(false);
        setProgress(null);
        refresh();
      });
    })();
    return () => {
      unProgress?.();
      unReady?.();
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
  // The selected variant is the one actually running only if it's loaded AND matches.
  const selectedActive = loaded && status?.loaded_variant === status?.selected;

  return (
    <div className="rounded-xl border border-[var(--border-subtle)] p-5">
      <div className="mb-1 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('GigaAM v3 (Russian)')}</h3>
          <p className="text-xs text-[var(--fg3)]">
            {t('Sber ASR · punctuated & capitalized · fully local')}
          </p>
        </div>
        {selectedActive ? (
          <span className="flex items-center gap-1 rounded-full bg-[color-mix(in_srgb,var(--success)_12%,transparent)] px-2 py-0.5 text-xs font-medium text-[var(--success)]">
            <CheckCircle2 className="h-3.5 w-3.5" /> {t('Active')}
          </span>
        ) : present ? (
          <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-xs text-[var(--fg2)]">{t('Installed — restart to load')}</span>
        ) : (
          <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-xs text-[var(--fg2)]">{t('Not downloaded')}</span>
        )}
      </div>

      {/* Variant selector for A/B quality testing */}
      <div className="mt-4">
        <label className="mb-1 block text-xs font-medium text-[var(--fg2)]">{t('Model variant')}</label>
        <select
          value={status?.selected ?? 'e2e-rnnt-fp32'}
          onChange={(e) => selectVariant(e.target.value)}
          disabled={!status || downloading || switching}
          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] px-3 py-2 text-sm text-[var(--fg1)] focus:border-[var(--gold-border)] focus:outline-none disabled:opacity-50"
        >
          {(status?.variants ?? []).map((v) => (
            <option key={v.id} value={v.id}>
              {v.label} · ~{v.size_mb}{t(' MB')}{v.present ? t(' · installed') : ''}
            </option>
          ))}
        </select>
        <p className="mt-1 text-xs text-[var(--fg3)]">
          {t('RNN-T usually beats CTC on accuracy; fp32 avoids int8 quantization loss (larger & slower).')}
        </p>
      </div>

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
        ) : switching ? (
          <div className="flex items-center gap-2 text-sm text-[var(--fg2)]">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('Loading')} {selected?.label ?? t('variant')}…
          </div>
        ) : selectedActive ? (
          <div className="flex items-center gap-2 text-sm font-medium text-[var(--success)]">
            <CheckCircle2 className="h-4 w-4" />
            {t('Ready — recordings will transcribe with this variant')}
          </div>
        ) : (
          <button
            onClick={download}
            className="flex items-center gap-2 rounded-lg bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)]"
          >
            <Download className="h-4 w-4" />
            {present ? t('Re-download variant') : `${t('Download variant (~')}${selected?.size_mb ?? '?'}${t(' MB)')}`}
          </button>
        )}

        {/* Loaded-vs-selected mismatch hint (e.g. selected a new variant that still needs a download) */}
        {!switching && !downloading && loaded && !selectedActive && (
          <p className="mt-2 text-xs text-[var(--gold)]">
            {t('Currently running')} <span className="font-medium">{status?.loaded_variant}</span>.
            {present ? t(' Restart to load the selected variant.') : t(' Download the selected variant to switch.')}
          </p>
        )}

        {error && (
          <div className="mt-3 flex items-start gap-1.5 text-sm text-[var(--danger)]">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
