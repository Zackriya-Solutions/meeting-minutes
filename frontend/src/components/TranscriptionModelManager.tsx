'use client';

/**
 * Transcription model manager.
 *
 * Replaces WhisperModelManager and ParakeetModelManager, which were the same
 * component against two engines. One engine now means one catalog and one list.
 */

import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  MODEL_SORT_LABELS,
  ModelInfo,
  ModelSort,
  TranscribeAPI,
  downloadProgress,
  formatFileSize,
  getModelIcon,
  getModelLabel,
  getModelUseTag,
  isDownloading,
  sortModels,
} from '@/lib/transcribe';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

interface Props {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
}

export default function TranscriptionModelManager({ selectedModel, onModelSelect }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [sort, setSort] = useState<ModelSort>('catalog');

  const refresh = useCallback(async () => {
    try {
      setModels(await TranscribeAPI.getAvailableModels());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Download progress arrives as events; re-read the list on each terminal event
  // rather than mirroring per-model progress into local state.
  useEffect(() => {
    const unlisteners = [
      listen<{ modelName: string; progress: number }>('model-download-progress', (e) => {
        setModels((prev) =>
          prev.map((m) =>
            m.name === e.payload.modelName
              ? { ...m, status: { Downloading: { progress: e.payload.progress } } }
              : m
          )
        );
      }),
      listen('model-download-complete', () => {
        setBusy(null);
        refresh();
      }),
      listen<{ error: string }>('model-download-error', (e) => {
        setBusy(null);
        setError(e.payload.error);
        refresh();
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [refresh]);

  const download = async (name: string) => {
    setBusy(name);
    setError(null);
    try {
      await TranscribeAPI.downloadModel(name);
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  };

  const remove = async (name: string) => {
    try {
      await TranscribeAPI.deleteCorruptedModel(name);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const card = (model: ModelInfo) => {
    const available = model.status === 'Available';
    const downloading = isDownloading(model.status);
    const selected = selectedModel === model.name;

    // Selection is a brand border, never a filled surface — the fill is what
    // made a selected card read as a status callout. See /DESIGN.md.
    return (
      <div
        key={model.name}
        className={`rounded-lg border p-4 transition-colors ${
          selected ? 'border-brand' : 'border-line'
        }`}
      >
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span>{getModelIcon(model.accuracy)}</span>
              <span className="font-medium">{getModelLabel(model.name)}</span>
              <span
                className={`rounded-full px-2 py-0.5 text-xs ${
                  model.streaming
                    ? 'bg-brand-soft text-brand-soft-ink'
                    : 'bg-warn-soft text-warn-ink'
                }`}
              >
                {getModelUseTag(model)}
              </span>
            </div>
            <p className="mt-1 text-sm text-ink-muted">{model.description}</p>
            <dl className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-muted">
              <div className="flex gap-1">
                <dt>Quality</dt>
                <dd className="font-medium text-ink">{model.accuracy}</dd>
              </div>
              <div className="flex gap-1">
                <dt>Speed</dt>
                <dd className="font-medium text-ink">{model.speed}</dd>
              </div>
              <div className="flex gap-1">
                <dt>Download</dt>
                <dd className="readout text-ink">{formatFileSize(model.size_mb)}</dd>
              </div>
              <div className="flex gap-1">
                <dt className="sr-only">Languages</dt>
                <dd>{model.languages}</dd>
              </div>
            </dl>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            {available && !selected && (
              <Button size="sm" onClick={() => onModelSelect?.(model.name)}>
                Use
              </Button>
            )}
            {available && selected && (
              <span className="text-sm font-medium text-brand">Selected</span>
            )}
            {!available && !downloading && (
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null}
                onClick={() => download(model.name)}
              >
                Download
              </Button>
            )}
            {available && (
              <Button size="sm" variant="ghost" onClick={() => remove(model.name)}>
                Delete
              </Button>
            )}
          </div>
        </div>

        {downloading && (
          <div className="mt-3">
            <div className="h-2 w-full overflow-hidden rounded-full bg-ink/10">
              <div
                className="h-full bg-brand transition-all"
                style={{ width: `${downloadProgress(model.status)}%` }}
              />
            </div>
            <p className="mt-1 text-xs text-ink-muted">
              Downloading… {downloadProgress(model.status)}%
            </p>
          </div>
        )}
      </div>
    );
  };

  if (loading) {
    return <div className="text-sm text-ink-muted">Loading models…</div>;
  }

  // A downloaded or selected model must stay reachable even when it is not on the
  // recommended list, or the user cannot see what they are currently using
  // without expanding the whole catalog.
  const isPinned = (m: ModelInfo) =>
    m.recommended || m.name === selectedModel || m.status === 'Available';
  const pinned = sortModels(models.filter(isPinned), sort);
  const rest = models.filter((m) => !isPinned(m));

  // Insertion order is the catalog's order, which is live-capable first.
  const families = new Map<string, ModelInfo[]>();
  for (const model of rest) {
    const group = families.get(model.family);
    if (group) group.push(model);
    else families.set(model.family, [model]);
  }

  return (
    <div className="space-y-3">
      {error && (
        <div className="rounded-md bg-danger-soft p-3 text-sm text-danger-ink">{error}</div>
      )}

      <div className="flex flex-wrap items-start justify-between gap-3">
        <p className="max-w-[52ch] text-xs text-ink-muted">
          Quality is a tier from each model&apos;s published error rate, and speed is
          estimated from its size. Both are catalog estimates — reliable for picking
          a tier, not for ranking two models against each other.
        </p>
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-sm text-ink-muted">Sort by</span>
          <Select value={sort} onValueChange={(v) => setSort(v as ModelSort)}>
            <SelectTrigger className="h-8 w-40" aria-label="Sort models by">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(MODEL_SORT_LABELS).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {pinned.map(card)}

      {rest.length > 0 && (
        <div className="pt-1">
          <button
            type="button"
            className="text-sm font-medium text-ink hover:text-ink"
            onClick={() => setShowAll((v) => !v)}
            aria-expanded={showAll}
          >
            {showAll ? '▾' : '▸'} All models ({rest.length})
          </button>

          {showAll && (
            <div className="mt-3 space-y-5">
              {[...families.entries()].map(([family, group]) => (
                <div key={family} className="space-y-3">
                  <h4 className="text-xs font-semibold uppercase tracking-wide text-ink-muted">
                    {family}
                  </h4>
                  {sortModels(group, sort).map(card)}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <button
        type="button"
        className="text-xs text-ink-muted underline"
        onClick={() => TranscribeAPI.openModelsFolder()}
      >
        Open models folder
      </button>
    </div>
  );
}
