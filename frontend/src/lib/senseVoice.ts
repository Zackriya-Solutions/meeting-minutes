import { invoke } from '@tauri-apps/api/core';

export const SENSE_VOICE_MODEL = 'sense-voice-small-int8';

export type SenseVoiceModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: { progress: number } }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_size: number } };

export interface SenseVoiceModelInfo {
  name: string;
  path: string;
  size_mb: number;
  status: SenseVoiceModelStatus;
  description: string;
}

export const SenseVoiceAPI = {
  init: () => invoke<void>('sense_voice_init'),
  getAvailableModels: () => invoke<SenseVoiceModelInfo[]>('sense_voice_get_available_models'),
  downloadModel: (modelName: string) =>
    invoke<void>('sense_voice_download_model', { modelName }),
  cancelDownload: () => invoke<void>('sense_voice_cancel_download'),
  deleteModel: () => invoke<void>('sense_voice_delete_model'),
  openModelsFolder: () => invoke<void>('open_sense_voice_models_folder'),
};

export function senseVoiceDownloadProgress(status: SenseVoiceModelStatus): number | null {
  if (typeof status === 'object' && 'Downloading' in status) {
    return status.Downloading.progress;
  }
  return null;
}
