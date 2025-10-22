// Types for Parakeet (NVIDIA NeMo) integration
export interface ParakeetModelInfo {
  name: string;
  path: string;
  size_mb: number;
  accuracy: ModelAccuracy;
  speed: ProcessingSpeed;
  status: ModelStatus;
  description?: string;
  quantization: QuantizationType;
}

export type QuantizationType = 'FP32' | 'Int8';
export type ModelAccuracy = 'High' | 'Good' | 'Decent';
export type ProcessingSpeed = 'Slow' | 'Medium' | 'Fast' | 'Very Fast' | 'Ultra Fast';

export type ModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: number }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_min_size: number } };

export interface ParakeetEngineState {
  currentModel: string | null;
  availableModels: ParakeetModelInfo[];
  isLoading: boolean;
  error: string | null;
}

// User-friendly model display configuration
export interface ModelDisplayInfo {
  friendlyName: string;
  icon: string;
  tagline: string;
  recommended?: boolean;
  tier: 'fastest' | 'balanced' | 'precise';
}

export const MODEL_DISPLAY_CONFIG: Record<string, ModelDisplayInfo> = {
  'parakeet-tdt-0.6b-v3-int8': {
    friendlyName: 'Lightning',
    icon: '⚡',
    tagline: 'Real time • Best for speed, great accuracy',
    recommended: true,
    tier: 'fastest'
  },
  'parakeet-tdt-0.6b-v2-int8': {
    friendlyName: 'Compact',
    icon: '📦',
    tagline: 'Real time • Smaller size',
    tier: 'balanced'
  },
  'parakeet-tdt-0.6b-v3-fp32': {
    friendlyName: 'Precise',
    icon: '🎯',
    tagline: '20x real-time • Higher accuracy',
    tier: 'precise'
  }
};

// Model configuration for Parakeet models (matching Rust implementation)
// Supported models: parakeet-tdt-0.6b in v2 and v3 variants
// Source: https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx
export const PARAKEET_MODEL_CONFIGS: Record<string, Partial<ParakeetModelInfo>> = {
  'parakeet-tdt-0.6b-v3-int8': {
    description: 'Real time on M4 Max, optimized for speed',
    size_mb: 670, // Actual download: 652MB encoder + 18.2MB decoder + 0.2MB extras
    accuracy: 'High',
    speed: 'Ultra Fast',
    quantization: 'Int8'
  },
  'parakeet-tdt-0.6b-v2-int8': {
    description: '25x real-time, smaller size with good accuracy',
    size_mb: 661, // Actual download: 652MB encoder + 9MB decoder + 0.15MB extras
    accuracy: 'High',
    speed: 'Very Fast',
    quantization: 'Int8'
  },
  'parakeet-tdt-0.6b-v3-fp32': {
    description: '20x real-time on M4 Max, higher precision',
    size_mb: 2554, // Actual download: 2.44GB + 41.8MB encoder + 72.5MB decoder + 0.2MB extras
    accuracy: 'High',
    speed: 'Fast',
    quantization: 'FP32'
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

// Get user-friendly display name for a model
export function getModelDisplayName(modelName: string): string {
  const displayInfo = MODEL_DISPLAY_CONFIG[modelName];
  return displayInfo?.friendlyName || modelName;
}

// Get model display info (icon, tagline, etc.)
export function getModelDisplayInfo(modelName: string): ModelDisplayInfo | null {
  return MODEL_DISPLAY_CONFIG[modelName] || null;
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

// Helper function to check if model is quantized
export function isQuantizedModel(modelName: string): boolean {
  return modelName.includes('int8');
}

// Helper function to get model performance badge
export function getModelPerformanceBadge(quantization: QuantizationType): { label: string; color: string } {
  switch (quantization) {
    case 'FP32':
      return { label: 'Full Precision', color: 'blue' };
    case 'Int8':
      return { label: 'Int8 Quantized', color: 'green' };
    default:
      return { label: 'Standard', color: 'gray' };
  }
}

export function getRecommendedModel(systemSpecs?: { ram: number; cores: number }): string {
  // Default to Int8 quantized model (fastest)
  if (!systemSpecs) return 'parakeet-tdt-0.6b-v3-int8';

  // For any system, prefer Int8 for speed
  // FP32 can be used if user explicitly wants higher precision
  return 'parakeet-tdt-0.6b-v3-int8';
}

// Tauri command wrappers for Parakeet backend
import { invoke } from '@tauri-apps/api/core';

export class ParakeetAPI {
  static async init(): Promise<void> {
    await invoke('parakeet_init');
  }

  static async getAvailableModels(): Promise<ParakeetModelInfo[]> {
    return await invoke('parakeet_get_available_models');
  }

  static async loadModel(modelName: string): Promise<void> {
    await invoke('parakeet_load_model', { modelName });
  }

  static async getCurrentModel(): Promise<string | null> {
    return await invoke('parakeet_get_current_model');
  }

  static async isModelLoaded(): Promise<boolean> {
    return await invoke('parakeet_is_model_loaded');
  }

  static async transcribeAudio(audioData: number[]): Promise<string> {
    return await invoke('parakeet_transcribe_audio', { audioData });
  }

  static async getModelsDirectory(): Promise<string> {
    return await invoke('parakeet_get_models_directory');
  }

  static async downloadModel(modelName: string): Promise<void> {
    await invoke('parakeet_download_model', { modelName });
  }

  static async cancelDownload(modelName: string): Promise<void> {
    await invoke('parakeet_cancel_download', { modelName });
  }

  static async deleteCorruptedModel(modelName: string): Promise<string> {
    return await invoke('parakeet_delete_corrupted_model', { modelName });
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke('parakeet_has_available_models');
  }

  static async validateModelReady(): Promise<string> {
    return await invoke('parakeet_validate_model_ready');
  }

  static async openModelsFolder(): Promise<void> {
    await invoke('open_parakeet_models_folder');
  }
}
