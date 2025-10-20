// Types for whisper-rs integration
export interface ModelInfo {
  name: string;
  path: string;
  size_mb: number;
  accuracy: ModelAccuracy;
  speed: ProcessingSpeed;
  status: ModelStatus;
  description?: string;
}

export type ModelAccuracy = 'High' | 'Good' | 'Decent';
export type ProcessingSpeed = 'Slow' | 'Medium' | 'Fast' | 'Very Fast';

export type ModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: number }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_min_size: number } };

export interface ModelDownloadProgress {
  modelName: string;
  progress: number;
  totalBytes: number;
  downloadedBytes: number;
  speed: string;
}

export interface WhisperEngineState {
  currentModel: string | null;
  availableModels: ModelInfo[];
  isLoading: boolean;
  error: string | null;
}

// Tauri command interfaces
export interface DownloadModelRequest {
  modelName: string;
}

export interface SwitchModelRequest {
  modelName: string;
}

export interface TranscribeAudioRequest {
  audioData: number[];
  sampleRate: number;
}

// Model configuration for different use cases
export const MODEL_CONFIGS: Record<string, Partial<ModelInfo>> = {
  // Standard f16 models (full precision)
  'large-v3': {
    description: 'Highest accuracy, best for important meetings. Slower processing.',
    size_mb: 2870,
    accuracy: 'High',
    speed: 'Slow'
  },
  'large-v3-turbo': {
    description: 'Best accuracy with improved speed.',
    size_mb: 809,
    accuracy: 'High',
    speed: 'Medium'
  },
  'medium': {
    description: 'Balanced accuracy and speed. Good for most use cases.',
    size_mb: 1420,
    accuracy: 'High',
    speed: 'Slow'
  },
  'small': {
    description: 'Fast processing with good quality. Great for quick transcription.',
    size_mb: 466,
    accuracy: 'Good',
    speed: 'Medium'
  },
  'base': {
    description: 'Good balance of speed and accuracy.',
    size_mb: 142,
    accuracy: 'Good',
    speed: 'Fast'
  },
  'tiny': {
    description: 'Fastest processing, good for real-time use.',
    size_mb: 39,
    accuracy: 'Decent',
    speed: 'Very Fast'
  },

  // Q5_0 quantized models (balanced speed/accuracy)
  'large-v3-q5_0': {
    description: 'Quantized large model, best balance of speed and accuracy.',
    size_mb: 1000,
    accuracy: 'High',
    speed: 'Medium'
  },
  'medium-q5_0': {
    description: 'Quantized medium model, professional quality with better speed.',
    size_mb: 852,
    accuracy: 'High',
    speed: 'Medium'
  },
  'small-q5_0': {
    description: 'Quantized small model, faster than f16 version.',
    size_mb: 280,
    accuracy: 'Good',
    speed: 'Fast'
  },
  'base-q5_0': {
    description: 'Quantized base model, good speed/accuracy balance.',
    size_mb: 85,
    accuracy: 'Good',
    speed: 'Fast'
  },
  'tiny-q5_0': {
    description: 'Quantized tiny model, ~50% faster processing.',
    size_mb: 26,
    accuracy: 'Decent',
    speed: 'Very Fast'
  },

  // Q4_0 quantized models (maximum speed)
  'medium-q4_0': {
    description: 'Fast medium model, good quality with maximum speed.',
    size_mb: 710,
    accuracy: 'High',
    speed: 'Fast'
  },
  'small-q4_0': {
    description: 'Fastest small model, rapid processing.',
    size_mb: 233,
    accuracy: 'Good',
    speed: 'Very Fast'
  },
  'base-q4_0': {
    description: 'Fastest base model, good for quick transcription.',
    size_mb: 71,
    accuracy: 'Good',
    speed: 'Very Fast'
  },
  'tiny-q4_0': {
    description: 'Fastest tiny model, some accuracy loss.',
    size_mb: 21,
    accuracy: 'Decent',
    speed: 'Very Fast'
  }
};

// Helper functions
export function getModelIcon(accuracy: ModelAccuracy): string {
  switch (accuracy) {
    case 'High': return '🔥';
    case 'Good': return '⚡';
    case 'Decent': return '🚀';
    default: return '📊';
  }
}

export function getStatusColor(status: ModelStatus): string {
  if (status === 'Available') return 'green';
  if (status === 'Missing') return 'gray';
  if (typeof status === 'object' && 'Downloading' in status) return 'blue';
  if (typeof status === 'object' && 'Error' in status) return 'red';
  return 'gray';
}

export function formatFileSize(sizeMb: number): string {
  if (sizeMb >= 1000) {
    return `${(sizeMb / 1000).toFixed(1)}GB`;
  }
  return `${sizeMb}MB`;
}

// Helper function to get model type (f16, q5_0, q4_0)
export function getModelType(modelName: string): 'f16' | 'q5_0' | 'q4_0' {
  if (modelName.includes('-q5_0')) return 'q5_0';
  if (modelName.includes('-q4_0')) return 'q4_0';
  return 'f16';
}

// Helper function to get model base name (without quantization suffix)
export function getModelBaseName(modelName: string): string {
  return modelName.replace(/-q[45]_0$/, '');
}

// Helper function to check if model is quantized
export function isQuantizedModel(modelName: string): boolean {
  return modelName.includes('-q');
}

// Helper function to get model performance badge
export function getModelPerformanceBadge(modelName: string): { label: string; color: string } {
  const type = getModelType(modelName);
  switch (type) {
    case 'f16':
      return { label: 'Full Precision', color: 'blue' };
    case 'q5_0':
      return { label: 'Balanced', color: 'green' };
    case 'q4_0':
      return { label: 'Fast', color: 'orange' };
    default:
      return { label: 'Standard', color: 'gray' };
  }
}

// Helper function to get concise tagline for model (similar to Parakeet style)
export function getModelTagline(modelName: string, speed: ProcessingSpeed, accuracy: ModelAccuracy): string {
  const isQuantized = isQuantizedModel(modelName);
  const baseName = getModelBaseName(modelName);

  // Speed prefix
  let speedText = '';
  switch (speed) {
    case 'Very Fast':
      speedText = 'Real time';
      break;
    case 'Fast':
      speedText = 'Fast processing';
      break;
    case 'Medium':
      speedText = 'Moderate speed';
      break;
    case 'Slow':
      speedText = 'Slower processing';
      break;
  }

  // Key feature based on model and accuracy
  let featureText = '';
  if (baseName === 'large-v3') {
    featureText = 'Highest accuracy';
  } else if (baseName === 'large-v3-turbo') {
    featureText = 'Best accuracy with speed';
  } else if (baseName === 'medium') {
    featureText = accuracy === 'High' ? 'Professional quality' : 'Balanced quality';
  } else if (baseName === 'small') {
    featureText = 'Good accuracy';
  } else if (baseName === 'base') {
    featureText = 'Balanced quality';
  } else if (baseName === 'tiny') {
    featureText = 'Fastest option';
  }

  // Add quantization note if applicable
  if (isQuantized) {
    const quantType = getModelType(modelName);
    if (quantType === 'q5_0') {
      featureText += ', optimized';
    } else if (quantType === 'q4_0') {
      featureText += ', ultra fast';
    }
  }

  return `${speedText} • ${featureText}`;
}

// Group models by their base name for better UI organization
export function groupModelsByBase(models: ModelInfo[]): Record<string, ModelInfo[]> {
  const grouped: Record<string, ModelInfo[]> = {};

  models.forEach(model => {
    const baseName = getModelBaseName(model.name);
    if (!grouped[baseName]) {
      grouped[baseName] = [];
    }
    grouped[baseName].push(model);
  });

  // Sort each group: f16 first, then q5_0, then q4_0
  Object.keys(grouped).forEach(baseName => {
    grouped[baseName].sort((a, b) => {
      const aType = getModelType(a.name);
      const bType = getModelType(b.name);
      const order = { 'f16': 0, 'q5_0': 1, 'q4_0': 2 };
      return order[aType] - order[bType];
    });
  });

  return grouped;
}

export function getRecommendedModel(systemSpecs?: { ram: number; cores: number }): string {
  if (!systemSpecs) return 'medium-q5_0'; // Default to balanced quantized model
  
  if (systemSpecs.ram >= 8000 && systemSpecs.cores >= 8) {
    return 'large-v3'; // High-end system
  } else if (systemSpecs.ram >= 4000 && systemSpecs.cores >= 4) {
    return 'medium'; // Mid-range system
  }
  return 'small'; // Lower-spec system
}

// Tauri command wrappers for whisper-rs backend
import { invoke } from '@tauri-apps/api/core';

export class WhisperAPI {
  static async init(): Promise<void> {
    await invoke('whisper_init');
  }
  
  static async getAvailableModels(): Promise<ModelInfo[]> {
    return await invoke('whisper_get_available_models');
  }
  
  static async loadModel(modelName: string): Promise<void> {
    await invoke('whisper_load_model', { modelName });
  }
  
  static async getCurrentModel(): Promise<string | null> {
    return await invoke('whisper_get_current_model');
  }
  
  static async isModelLoaded(): Promise<boolean> {
    return await invoke('whisper_is_model_loaded');
  }
  
  static async transcribeAudio(audioData: number[]): Promise<string> {
    return await invoke('whisper_transcribe_audio', { audioData });
  }
  
  static async getModelsDirectory(): Promise<string> {
    return await invoke('whisper_get_models_directory');
  }
  
  static async downloadModel(modelName: string): Promise<void> {
    await invoke('whisper_download_model', { modelName });
  }
  
  static async cancelDownload(modelName: string): Promise<void> {
    await invoke('whisper_cancel_download', { modelName });
  }

  static async deleteCorruptedModel(modelName: string): Promise<string> {
    return await invoke('whisper_delete_corrupted_model', { modelName });
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke('whisper_has_available_models');
  }

  static async validateModelReady(): Promise<string> {
    return await invoke('whisper_validate_model_ready');
  }

  static async openModelsFolder(): Promise<void> {
    await invoke('open_models_folder');
  }
}
