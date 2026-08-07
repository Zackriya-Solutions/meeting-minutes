/**
 * Transcription model management.
 *
 * Replaces lib/whisper.ts and lib/parakeet.ts, which were the same twelve
 * operations duplicated per engine. There is one local engine now
 * (transcribe.cpp), so there is one API surface and one model catalog.
 */

import { invoke } from '@tauri-apps/api/core';

export type ModelAccuracy = 'High' | 'Good' | 'Decent';
export type ProcessingSpeed = 'Slow' | 'Medium' | 'Fast' | 'Very Fast';

export type ModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: { progress: number } }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_min_size: number } };

export interface ModelInfo {
  name: string;
  path: string;
  size_mb: number;
  accuracy: ModelAccuracy;
  speed: ProcessingSpeed;
  status: ModelStatus;
  description: string;
  /** Can drive live recording without VAD segmentation. */
  streaming: boolean;
  /** Coverage blurb; the loaded model's own language list supersedes it. */
  languages: string;
  /** Display family, used to group the collapsed list. */
  family: string;
  /** Belongs above the "All models" disclosure. */
  recommended: boolean;
}

export interface ModelDownloadProgress {
  modelName: string;
  progress: number;
}

/**
 * Human-readable name for a catalog entry.
 *
 * Derived rather than looked up: the catalog is generated from transcribe.cpp's
 * model cards and holds ~86 entries, so a hand-maintained label map here would
 * be one more thing to forget when the pinned rev moves. Entry names are
 * `<variant>-<quant>` by construction.
 */
export function getModelLabel(modelName: string): string {
  const match = /^(.*)-(q4|q8)$/.exec(modelName);
  if (!match) return modelName;
  return `${match[1]} (${match[2].toUpperCase()})`;
}

/**
 * How this model behaves during live recording.
 *
 * Every catalog model can record live — batch-only families fall back to VAD
 * segmentation with one decode per utterance. So this is a latency distinction,
 * not a capability one: streaming models show text as it is spoken, batch models
 * show a whole sentence once the speaker pauses.
 */
export function getModelUseTag(model: Pick<ModelInfo, 'streaming'>): string {
  return model.streaming ? 'Live, as you speak' : 'Live, per sentence';
}

/**
 * Ordering for the picker's sort control. Higher is better.
 *
 * Both scales are the three/four buckets the Rust catalog generator emits, not
 * measurements: accuracy is a WER bucket that is not comparable across families
 * (a Russian set vs. LibriSpeech), and speed is derived from file size. Good
 * enough to order a list, not to claim model A beats model B.
 */
const ACCURACY_RANK: Record<ModelAccuracy, number> = { High: 3, Good: 2, Decent: 1 };
const SPEED_RANK: Record<ProcessingSpeed, number> = {
  'Very Fast': 4,
  Fast: 3,
  Medium: 2,
  Slow: 1,
};

export type ModelSort = 'catalog' | 'quality' | 'speed' | 'size';

export const MODEL_SORT_LABELS: Record<ModelSort, string> = {
  catalog: 'Recommended',
  quality: 'Quality',
  speed: 'Speed',
  size: 'Smallest first',
};

type SortableModel = Pick<ModelInfo, 'accuracy' | 'speed' | 'size_mb'>;

/**
 * Sorts a copy. `catalog` is the Rust catalog's own order (live-capable first),
 * and because Array.sort is stable it is also the tiebreak for every other mode —
 * so equally-ranked rows never shuffle between renders.
 */
export function sortModels<T extends SortableModel>(models: T[], sort: ModelSort): T[] {
  if (sort === 'catalog') return models;
  const compare: Record<Exclude<ModelSort, 'catalog'>, (a: T, b: T) => number> = {
    quality: (a, b) => ACCURACY_RANK[b.accuracy] - ACCURACY_RANK[a.accuracy],
    speed: (a, b) => SPEED_RANK[b.speed] - SPEED_RANK[a.speed],
    size: (a, b) => a.size_mb - b.size_mb,
  };
  return [...models].sort(compare[sort]);
}

export function getModelIcon(accuracy: ModelAccuracy): string {
  switch (accuracy) {
    case 'High':
      return '🎯';
    case 'Good':
      return '✨';
    default:
      return '⚡';
  }
}

export function getStatusColor(status: ModelStatus): string {
  if (status === 'Available') return 'text-brand';
  if (status === 'Missing') return 'text-ink-faint';
  if (typeof status === 'object' && 'Downloading' in status) return 'text-info-ink';
  return 'text-danger-ink';
}

export function formatFileSize(sizeMb: number): string {
  return sizeMb >= 1024 ? `${(sizeMb / 1024).toFixed(1)} GB` : `${sizeMb} MB`;
}

export function isDownloading(status: ModelStatus): boolean {
  return typeof status === 'object' && 'Downloading' in status;
}

export function downloadProgress(status: ModelStatus): number {
  return typeof status === 'object' && 'Downloading' in status
    ? status.Downloading.progress
    : 0;
}

export class TranscribeAPI {
  static async init(): Promise<void> {
    await invoke('transcribe_init');
  }

  static async getAvailableModels(): Promise<ModelInfo[]> {
    return await invoke('transcribe_get_available_models');
  }

  static async loadModel(modelName: string): Promise<void> {
    await invoke('transcribe_load_model', { modelName });
  }

  static async getCurrentModel(): Promise<string | null> {
    return await invoke('transcribe_get_current_model');
  }

  static async isModelLoaded(): Promise<boolean> {
    return await invoke('transcribe_is_model_loaded');
  }

  static async transcribeAudio(audioData: number[], language?: string): Promise<string> {
    return await invoke('transcribe_transcribe_audio', { audioData, language });
  }

  static async getModelsDirectory(): Promise<string> {
    return await invoke('transcribe_get_models_directory');
  }

  static async downloadModel(modelName: string): Promise<void> {
    await invoke('transcribe_download_model', { modelName });
  }

  static async cancelDownload(modelName: string): Promise<void> {
    await invoke('transcribe_cancel_download', { modelName });
  }

  static async deleteCorruptedModel(modelName: string): Promise<void> {
    await invoke('transcribe_delete_corrupted_model', { modelName });
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke('transcribe_has_available_models');
  }

  static async validateModelReady(): Promise<string> {
    return await invoke('transcribe_validate_model_ready');
  }

  static async openModelsFolder(): Promise<void> {
    await invoke('open_models_folder');
  }
}
