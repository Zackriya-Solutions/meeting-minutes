import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface KnowledgeReadiness {
  loading: boolean;
  indexableMeetings: number;
  searchableMeetings: number;
  chunksTotal: number;
  semanticCoverage: number;
  semanticEnabled: boolean;
  indexingActive: boolean;
  chatAllowed: boolean;
  localOnly: boolean;
  error: string | null;
}

interface EmbedderStatus {
  loaded?: boolean;
}

interface IndexingStatus {
  indexable_meetings?: number;
  chunked_meetings?: number;
  chunks_total?: number;
  embeddings_done?: number;
  queued_jobs?: number;
  running_jobs?: number;
}

const initialState: KnowledgeReadiness = {
  loading: true,
  indexableMeetings: 0,
  searchableMeetings: 0,
  chunksTotal: 0,
  semanticCoverage: 0,
  semanticEnabled: false,
  indexingActive: false,
  chatAllowed: true,
  localOnly: false,
  error: null,
};

const enabledSetting = (value: string | undefined, fallback: boolean): boolean => {
  if (value == null) return fallback;
  return value === 'true' || value === '1';
};

export function useKnowledgeReadiness(): KnowledgeReadiness & { refresh: () => void } {
  const [state, setState] = useState<KnowledgeReadiness>(initialState);

  const refresh = useCallback(() => {
    Promise.all([
      invoke<EmbedderStatus>('embedder_status').catch(() => null),
      invoke<IndexingStatus>('indexing_status'),
      invoke<Record<string, string>>('get_app_settings').catch(
        (): Record<string, string> => ({}),
      ),
    ])
      .then(([embedder, index, settings]) => {
        const chunksTotal = Math.max(0, Number(index.chunks_total) || 0);
        const embeddingsDone = Math.max(0, Number(index.embeddings_done) || 0);
        const localOnly = enabledSetting(settings['privacy.local_only'], false);
        const chatEnabled = enabledSetting(settings['privacy.chat_enabled'], true);
        setState({
          loading: false,
          indexableMeetings: Math.max(0, Number(index.indexable_meetings) || 0),
          searchableMeetings: Math.max(0, Number(index.chunked_meetings) || 0),
          chunksTotal,
          semanticCoverage: chunksTotal > 0
            ? Math.min(100, Math.round((embeddingsDone / chunksTotal) * 100))
            : 0,
          semanticEnabled: !!embedder?.loaded,
          indexingActive:
            (Number(index.queued_jobs) || 0) + (Number(index.running_jobs) || 0) > 0,
          chatAllowed: !localOnly && chatEnabled,
          localOnly,
          error: null,
        });
      })
      .catch((reason) => {
        setState((current) => ({
          ...current,
          loading: false,
          error: typeof reason === 'string' ? reason : 'Knowledge index status unavailable',
        }));
      });
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return { ...state, refresh };
}
