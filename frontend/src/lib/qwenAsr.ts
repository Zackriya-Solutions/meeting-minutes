import { invoke } from '@tauri-apps/api/core';

export const QWEN3_ASR_MODEL = 'qwen3-asr-0.6b-int8';
export const QWEN3_ASR_1_7B_MODEL = 'qwen3-asr-1.7b-int8';

export interface QwenModelDisplayInfo {
  name: string;
  icon: string;
  tagline: string;
  recommended?: boolean;
}

export const QWEN_MODEL_DISPLAY: Record<string, QwenModelDisplayInfo> = {
  [QWEN3_ASR_MODEL]: {
    name: 'Qwen3-ASR 0.6B Int8',
    icon: '🌐',
    tagline: 'Compact multilingual recognition with Chinese dialect support',
    recommended: true,
  },
  [QWEN3_ASR_1_7B_MODEL]: {
    name: 'Qwen3-ASR 1.7B Int8',
    icon: '🧠',
    tagline: 'Higher-capacity multilingual recognition with improved accuracy',
  },
};

export type QwenModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: { progress: number } }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_size: number } };

export interface QwenModelInfo {
  name: string;
  path: string;
  size_mb: number;
  status: QwenModelStatus;
  description: string;
}

export const QwenAsrAPI = {
  init: () => invoke<void>('qwen_asr_init'),
  getAvailableModels: () => invoke<QwenModelInfo[]>('qwen_asr_get_available_models'),
  downloadModel: (modelName: string) =>
    invoke<void>('qwen_asr_download_model', { modelName }),
  cancelDownload: () => invoke<void>('qwen_asr_cancel_download'),
  deleteModel: (modelName: string) =>
    invoke<void>('qwen_asr_delete_model', { modelName }),
  openModelsFolder: () => invoke<void>('open_qwen_asr_models_folder'),
};

export function qwenDownloadProgress(status: QwenModelStatus): number | null {
  if (typeof status !== 'object' || !('Downloading' in status)) return null;
  return status.Downloading.progress;
}
