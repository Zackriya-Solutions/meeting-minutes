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
  ModelInfo,
  TranscribeAPI,
  downloadProgress,
  formatFileSize,
  getModelIcon,
  getModelLabel,
  getModelUseTag,
  isDownloading,
} from '@/lib/transcribe';
import { Button } from '@/components/ui/button';

interface Props {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
  autoSave?: boolean;
}

export default function TranscriptionModelManager({ selectedModel, onModelSelect }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);

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

    return (
      <div
        key={model.name}
        className={`rounded-lg border p-4 transition-colors ${
          selected ? 'border-info/40 bg-info-soft' : 'border-line'
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
                    ? 'bg-brand-soft text-brand'
                    : 'bg-warn-soft text-warn-ink'
                }`}
              >
                {getModelUseTag(model)}
              </span>
            </div>
            <p className="mt-1 text-sm text-ink-muted">{model.description}</p>
            <p className="mt-1 text-xs text-ink-muted">
              {model.languages} · {formatFileSize(model.size_mb)} · {model.speed}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            {available && !selected && (
              <Button size="sm" onClick={() => onModelSelect?.(model.name)}>
                Use
              </Button>
            )}
            {available && selected && (
              <span className="text-sm font-medium text-info-ink">Selected</span>
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
  const pinned = models.filter(isPinned);
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
                  {group.map(card)}
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
