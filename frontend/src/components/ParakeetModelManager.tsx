import React, { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import {
  ParakeetModelInfo,
  ModelStatus,
  getModelIcon,
  formatFileSize,
  getModelPerformanceBadge,
  isQuantizedModel,
  ParakeetAPI
} from '../lib/parakeet';
import { ModelDownloadProgress, ProgressRing } from './ModelDownloadProgress';

interface ParakeetModelManagerProps {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
  className?: string;
  autoSave?: boolean;
}

export function ParakeetModelManager({
  selectedModel,
  onModelSelect,
  className = '',
  autoSave = false
}: ParakeetModelManagerProps) {
  const [models, setModels] = useState<ParakeetModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [isAutoSaving, setIsAutoSaving] = useState(false);
  const [lastSavedModel, setLastSavedModel] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [hasUserSelection, setHasUserSelection] = useState(false);

  // Load persisted downloading state from localStorage
  const getPersistedDownloadingModels = (): Set<string> => {
    try {
      const saved = localStorage.getItem('downloading-parakeet-models');
      return saved ? new Set<string>(JSON.parse(saved) as string[]) : new Set<string>();
    } catch {
      return new Set<string>();
    }
  };

  const [downloadingModels, setDownloadingModels] = useState<Set<string>>(getPersistedDownloadingModels());

  // Persist downloading state to localStorage
  const updateDownloadingModels = (updater: (prev: Set<string>) => Set<string>) => {
    setDownloadingModels(prev => {
      const newSet = updater(prev);
      localStorage.setItem('downloading-parakeet-models', JSON.stringify(Array.from(newSet)));
      return newSet;
    });
  };

  // Lazy initialization - only load once
  useEffect(() => {
    if (initialized) return;

    const initializeModels = async () => {
      await loadAvailableModels();
      await syncDownloadStates();
      setInitialized(true);
    };
    initializeModels();
  }, []);

  // Check and sync download states
  const syncDownloadStates = async () => {
    try {
      const persistedDownloading = getPersistedDownloadingModels();

      setModels(prevModels => {
        const updatedModels = prevModels.map(model => {
          if (persistedDownloading.has(model.name)) {
            if (model.status === 'Available') {
              updateDownloadingModels(prev => {
                const newSet = new Set(prev);
                newSet.delete(model.name);
                return newSet;
              });
              console.log(`Parakeet download completed while app was closed: ${model.name}`);
            } else if (typeof model.status === 'object' && 'Corrupted' in model.status) {
              updateDownloadingModels(prev => {
                const newSet = new Set(prev);
                newSet.delete(model.name);
                return newSet;
              });
              console.log(`Parakeet download was interrupted and file is corrupted: ${model.name}`);
            } else if (model.status === 'Missing') {
              updateDownloadingModels(prev => {
                const newSet = new Set(prev);
                newSet.delete(model.name);
                return newSet;
              });
              console.log(`Parakeet download failed or incomplete: ${model.name}`);
            }
          }
          return model;
        });
        return updatedModels;
      });
    } catch (error) {
      console.error('Failed to sync Parakeet download states:', error);
    }
  };

  // Set up event listeners
  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;
    let unlistenModelLoadingStarted: (() => void) | null = null;
    let unlistenModelLoadingCompleted: (() => void) | null = null;
    let unlistenModelLoadingFailed: (() => void) | null = null;

    const setupListeners = async () => {
      console.log('Setting up Parakeet event listeners...');

      // Listen for model loading events
      unlistenModelLoadingStarted = await listen<{ modelName: string }>('parakeet-model-loading-started', (event) => {
        console.log('Parakeet model loading started:', event.payload.modelName);
        setLoading(true);
      });

      unlistenModelLoadingCompleted = await listen<{ modelName: string }>('parakeet-model-loading-completed', (event) => {
        console.log('Parakeet model loading completed:', event.payload.modelName);
        setLoading(false);
      });

      unlistenModelLoadingFailed = await listen<{ modelName: string; error: string }>('parakeet-model-loading-failed', (event) => {
        console.error('Parakeet model loading failed:', event.payload);
        setLoading(false);
        setError(`Failed to load model: ${event.payload.error}`);
      });

      // Listen for download progress updates
      unlistenProgress = await listen<{ modelName: string; progress: number }>('parakeet-model-download-progress', (event) => {
        console.log('Received parakeet-model-download-progress event:', event);
        const { modelName, progress } = event.payload;
        console.log(`Parakeet download progress for ${modelName}: ${progress}%`);

        setModels(prevModels => prevModels.map(model => {
          if (model.name === modelName) {
            const currentProgress = typeof model.status === 'object' && 'Downloading' in model.status
              ? model.status.Downloading
              : 0;
            const newProgress = Math.max(currentProgress, progress);

            return {
              ...model,
              status: { Downloading: newProgress } as ModelStatus
            };
          }
          return model;
        }));
      });

      // Listen for download completion
      unlistenComplete = await listen<{ modelName: string }>('parakeet-model-download-complete', (event) => {
        console.log('Received parakeet-model-download-complete event:', event);
        const { modelName } = event.payload;
        console.log(`Parakeet download completed for ${modelName}`);

        setModels(prevModels => prevModels.map(model => {
          if (model.name === modelName) {
            return {
              ...model,
              status: 'Available' as ModelStatus
            };
          }
          return model;
        }));

        updateDownloadingModels(prev => {
          const newSet = new Set(prev);
          newSet.delete(modelName);
          return newSet;
        });
      });

      // Listen for download errors
      unlistenError = await listen<{ modelName: string; error: string }>('parakeet-model-download-error', (event) => {
        console.log('Received parakeet-model-download-error event:', event);
        const { modelName, error } = event.payload;
        console.error(`Parakeet download failed for ${modelName}:`, error);

        setModels(prevModels => prevModels.map(model => {
          if (model.name === modelName) {
            return {
              ...model,
              status: { Error: error } as ModelStatus
            };
          }
          return model;
        }));

        updateDownloadingModels(prev => {
          const newSet = new Set(prev);
          newSet.delete(modelName);
          return newSet;
        });
      });
    };

    setupListeners();

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
      if (unlistenError) unlistenError();
      if (unlistenModelLoadingStarted) unlistenModelLoadingStarted();
      if (unlistenModelLoadingCompleted) unlistenModelLoadingCompleted();
      if (unlistenModelLoadingFailed) unlistenModelLoadingFailed();
    };
  }, []);

  const loadAvailableModels = async () => {
    if (isRefreshing) {
      console.log('Parakeet model refresh already in progress, skipping...');
      return;
    }

    try {
      setIsRefreshing(true);
      setLoading(true);
      setError(null);

      // Initialize Parakeet engine if not already done
      await ParakeetAPI.init();

      // Get model list from Parakeet backend
      const modelList = await ParakeetAPI.getAvailableModels();
      console.log('Parakeet models:', modelList);

      // Apply persisted downloading states
      const persistedDownloading = getPersistedDownloadingModels();
      const modelsWithDownloadState = modelList.map(model => {
        if (persistedDownloading.has(model.name) && model.status !== 'Available') {
          if (typeof model.status === 'object' && 'Corrupted' in model.status) {
            updateDownloadingModels(prev => {
              const newSet = new Set(prev);
              newSet.delete(model.name);
              return newSet;
            });
            console.log(`Parakeet download was interrupted and model is corrupted: ${model.name}`);
            return model;
          } else if (model.status === 'Missing') {
            updateDownloadingModels(prev => {
              const newSet = new Set(prev);
              newSet.delete(model.name);
              return newSet;
            });
            console.log(`Parakeet download was interrupted and model is missing: ${model.name}`);
            return model;
          } else {
            return {
              ...model,
              status: { Downloading: 0 } as ModelStatus
            };
          }
        }
        return model;
      });

      setModels(modelsWithDownloadState);

      // Auto-select first available model on initial load
      if (!hasUserSelection && !selectedModel) {
        const availableModel = modelsWithDownloadState.find(m => m.status === 'Available');
        if (availableModel && onModelSelect) {
          console.log(`Auto-selecting first available Parakeet model: ${availableModel.name}`);
          onModelSelect(availableModel.name);
        }
      }

      // Validate current selection
      if (selectedModel) {
        const currentModel = modelsWithDownloadState.find(m => m.name === selectedModel);
        if (!currentModel || currentModel.status !== 'Available') {
          console.log(`Selected Parakeet model "${selectedModel}" is no longer available, clearing selection`);
          if (onModelSelect) {
            onModelSelect('');
          }
        }
      }
    } catch (err) {
      console.error('Failed to load Parakeet models:', err);
      setError(err instanceof Error ? err.message : 'Failed to load models');
    } finally {
      setLoading(false);
      setIsRefreshing(false);
    }
  };

  const saveModelSelection = async (modelName: string) => {
    if (!autoSave || isAutoSaving) return;

    try {
      setIsAutoSaving(true);
      console.log(`Auto-saving Parakeet model selection: ${modelName}`);

      await invoke('api_save_transcript_config', {
        provider: 'parakeet',
        model: modelName,
        apiKey: null
      });

      setLastSavedModel(modelName);
      console.log(`Successfully auto-saved Parakeet model: ${modelName}`);
    } catch (error) {
      console.error('Failed to auto-save Parakeet model selection:', error);
    } finally {
      setIsAutoSaving(false);
    }
  };

  const downloadModel = async (modelName: string) => {
    if (downloadingModels.has(modelName)) {
      console.log(`Parakeet download already in progress for model: ${modelName}`);
      return;
    }

    try {
      console.log(`Starting Parakeet download for model: ${modelName}`);
      updateDownloadingModels(prev => new Set([...prev, modelName]));

      setModels(prevModels => prevModels.map(model => {
        if (model.name === modelName) {
          return {
            ...model,
            status: { Downloading: 0 } as ModelStatus
          };
        }
        return model;
      }));

      console.log(`Calling ParakeetAPI.downloadModel for: ${modelName}`);
      await ParakeetAPI.downloadModel(modelName);
      console.log(`Parakeet download completed for: ${modelName}`);

      await new Promise(resolve => setTimeout(resolve, 1000));

      // Verify the model was downloaded
      const updatedModels = await ParakeetAPI.getAvailableModels();
      const downloadedModel = updatedModels.find(m => m.name === modelName);

      if (downloadedModel?.status !== 'Available') {
        throw new Error(`Parakeet model download verification failed. Model status: ${JSON.stringify(downloadedModel?.status)}`);
      }

      updateDownloadingModels(prev => {
        const newSet = new Set(prev);
        newSet.delete(modelName);
        return newSet;
      });

      console.log(`Successfully downloaded and verified Parakeet model: ${modelName}`);

      // Auto-select the downloaded model
      if (onModelSelect) {
        console.log(`Auto-selecting downloaded Parakeet model: ${modelName}`);
        setHasUserSelection(true);
        onModelSelect(modelName);

        if (autoSave) {
          await saveModelSelection(modelName);
        }
      }
    } catch (err) {
      console.error('Failed to download Parakeet model:', err);

      const errorMessage = err instanceof Error ? err.message : 'Download failed';

      setModels(prev => prev.map(model =>
        model.name === modelName
          ? { ...model, status: { Error: errorMessage } }
          : model
      ));
      updateDownloadingModels(prev => {
        const newSet = new Set(prev);
        newSet.delete(modelName);
        return newSet;
      });

      console.error(`Parakeet model download failed for ${modelName}: ${errorMessage}`);
    }
  };

  const selectModel = async (modelName: string) => {
    console.log(`[ParakeetModelManager] User selected model: ${modelName}`);
    console.log(`[ParakeetModelManager] autoSave enabled: ${autoSave}`);
    console.log(`[ParakeetModelManager] onModelSelect callback exists: ${!!onModelSelect}`);

    setHasUserSelection(true);

    if (onModelSelect) {
      console.log(`[ParakeetModelManager] Calling onModelSelect callback for: ${modelName}`);
      onModelSelect(modelName);
    }

    if (autoSave) {
      console.log(`[ParakeetModelManager] Auto-save enabled, saving model: ${modelName}`);
      await saveModelSelection(modelName);
    } else {
      console.log(`[ParakeetModelManager] Auto-save disabled, parent should handle save`);
    }
  };

  const deleteCorruptedModel = async (modelName: string) => {
    try {
      console.log(`Attempting to delete corrupted Parakeet model: ${modelName}`);

      const confirmed = window.confirm(
        `Are you sure you want to delete the corrupted file for "${modelName}"? This cannot be undone.`
      );

      if (!confirmed) {
        return;
      }

      setModels(prevModels => prevModels.map(model => {
        if (model.name === modelName) {
          return {
            ...model,
            status: { Error: 'Deleting corrupted file...' } as ModelStatus
          };
        }
        return model;
      }));

      const result = await ParakeetAPI.deleteCorruptedModel(modelName);
      console.log(`Delete result: ${result}`);

      await loadAvailableModels();

      console.log(`Successfully deleted corrupted Parakeet model: ${modelName}`);
    } catch (err) {
      console.error('Failed to delete corrupted Parakeet model:', err);

      await loadAvailableModels();

      const errorMessage = err instanceof Error ? err.message : 'Delete failed';
      alert(`Failed to delete corrupted model: ${errorMessage}`);
    }
  };

  const getStatusBadge = (status: ModelStatus) => {
    if (status === 'Available') {
      return (
        <div className="flex items-center space-x-1">
          <div className="w-2 h-2 bg-green-500 rounded-full"></div>
          <span className="text-xs text-green-700 font-medium">Ready</span>
        </div>
      );
    } else if (status === 'Missing') {
      return (
        <div className="flex items-center space-x-1">
          <div className="w-2 h-2 bg-gray-400 rounded-full"></div>
          <span className="text-xs text-gray-600">Not Downloaded</span>
        </div>
      );
    } else if (typeof status === 'object' && 'Downloading' in status) {
      return <ProgressRing progress={status.Downloading} size={24} strokeWidth={2} />;
    } else if (typeof status === 'object' && 'Error' in status) {
      return (
        <div className="flex items-center space-x-1">
          <div className="w-2 h-2 bg-red-500 rounded-full"></div>
          <span className="text-xs text-red-700">Error</span>
        </div>
      );
    } else if (typeof status === 'object' && 'Corrupted' in status) {
      const { file_size, expected_min_size } = status.Corrupted;
      const fileSizeMB = (file_size / (1024 * 1024)).toFixed(1);
      const expectedSizeMB = (expected_min_size / (1024 * 1024)).toFixed(1);
      return (
        <div className="flex items-center space-x-1">
          <div className="w-2 h-2 bg-orange-500 rounded-full animate-pulse"></div>
          <span className="text-xs text-orange-700 font-medium" title={`File corrupted: ${fileSizeMB}MB (expected ≥${expectedSizeMB}MB)`}>
            Corrupted
          </span>
        </div>
      );
    }
    return null;
  };

  const availableModels = models.filter(m => m.status === 'Available');

  if (loading) {
    return (
      <div className={`animate-pulse space-y-4 ${className}`}>
        <div className="h-4 bg-gray-200 rounded w-3/4"></div>
        <div className="space-y-3">
          {[1, 2].map(i => (
            <div key={i} className="h-20 bg-gray-200 rounded-lg"></div>
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={`bg-red-50 border border-red-200 rounded-lg p-4 ${className}`}>
        <div className="flex items-center space-x-2">
          <span className="text-red-600">❌</span>
          <div>
            <h4 className="font-medium text-red-800">Failed to load Parakeet models</h4>
            <p className="text-sm text-red-700">{error}</p>
          </div>
        </div>
        <button
          onClick={loadAvailableModels}
          className="mt-3 text-sm text-red-600 hover:text-red-800 underline"
        >
          Try again
        </button>
      </div>
    );
  }

  return (
    <div className={`space-y-4 ${className}`}>
      <div className="grid gap-4 overflow-y-auto">
        {models.map((model) => {
          const isSelected = selectedModel === model.name;
          const isDownloading = typeof model.status === 'object' && 'Downloading' in model.status;
          const isAvailable = model.status === 'Available';

          return (
            <div key={model.name} className="space-y-2">
              <div
                className={`p-4 border rounded-lg transition-all ${
                  isAvailable ? 'cursor-pointer' : 'cursor-not-allowed'
                } ${
                  isSelected
                    ? 'border-blue-500 bg-blue-50 shadow-sm'
                    : isAvailable
                      ? 'border-gray-200 hover:border-gray-300 hover:shadow-sm'
                      : 'border-gray-200'
                } ${!isAvailable && !isDownloading ? 'opacity-75' : ''}`}
                onClick={() => isAvailable && selectModel(model.name)}
              >
                <div className="flex justify-between items-start">
                  <div className="flex-1 pr-4">
                    <div className="flex items-center space-x-3 mb-2">
                      <span className="text-2xl">{getModelIcon(model.accuracy)}</span>
                      <div>
                        <h4 className="font-medium text-gray-900 flex items-center space-x-2">
                          <span>Parakeet {model.name}</span>
                          {isSelected && (
                            <span className="bg-blue-600 text-white px-2 py-1 rounded-full text-xs flex items-center gap-1">
                              {lastSavedModel === model.name && autoSave && (
                                <span className="text-white">✓</span>
                              )}
                              Active
                            </span>
                          )}
                          {isAutoSaving && selectedModel === model.name && (
                            <span className="text-xs text-gray-500 animate-pulse">Saving...</span>
                          )}
                          <span className={`px-2 py-1 rounded-full text-xs ${
                            getModelPerformanceBadge(model.quantization).color === 'green'
                              ? 'bg-green-100 text-green-700'
                              : 'bg-blue-100 text-blue-700'
                          }`}>
                            {getModelPerformanceBadge(model.quantization).label}
                          </span>
                        </h4>
                        <p className="text-xs text-gray-600 mt-1">{model.description}</p>
                      </div>
                    </div>

                    <div className="flex items-center space-x-4 text-sm text-gray-600">
                      <span className="flex items-center space-x-1">
                        <span>📦</span>
                        <span>{formatFileSize(model.size_mb)}</span>
                      </span>
                      <span className="flex items-center space-x-1">
                        <span>🎯</span>
                        <span>{model.accuracy} accuracy</span>
                      </span>
                      <span className="flex items-center space-x-1">
                        <span>⚡</span>
                        <span>{model.speed} processing</span>
                      </span>
                    </div>
                  </div>

                  <div className="flex flex-col items-end space-y-2">
                    {getStatusBadge(model.status)}

                    {model.status === 'Missing' && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          downloadModel(model.name);
                        }}
                        className="bg-blue-600 text-white px-3 py-1 rounded text-xs hover:bg-blue-700 transition-colors"
                      >
                        Download
                      </button>
                    )}

                    {typeof model.status === 'object' && 'Error' in model.status && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          downloadModel(model.name);
                        }}
                        className="bg-red-600 text-white px-3 py-1 rounded text-xs hover:bg-red-700 transition-colors"
                      >
                        Retry
                      </button>
                    )}

                    {(typeof model.status === 'object' && 'Corrupted' in model.status) && (
                      <div className="flex flex-col space-y-1">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteCorruptedModel(model.name);
                          }}
                          title="Delete the corrupted model file"
                          className="bg-orange-600 text-white px-3 py-1 rounded text-xs hover:bg-orange-700 transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            downloadModel(model.name);
                          }}
                          title="Download the model again"
                          className="bg-blue-600 text-white px-3 py-1 rounded text-xs hover:bg-blue-700 transition-colors"
                        >
                          Re-download
                        </button>
                      </div>
                    )}

                    {model.status === 'Available' && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteCorruptedModel(model.name);
                        }}
                        title="Delete this model to free up space"
                        className="bg-red-600 text-white px-3 py-1 rounded text-xs hover:bg-red-700 transition-colors"
                      >
                        Delete
                      </button>
                    )}
                  </div>
                </div>
              </div>

              {isDownloading && (
                <ModelDownloadProgress
                  status={model.status}
                  modelName={model.name}
                />
              )}
            </div>
          );
        })}
      </div>

      {selectedModel && availableModels.length > 0 && (
        <div className="bg-green-50 border border-green-200 rounded-lg p-3">
          <div className="flex items-center space-x-2">
            <span className="text-green-600">✓</span>
            <p className="text-sm text-green-800">
              Using <strong>{selectedModel}</strong> Parakeet model for transcription (up to 30x faster!)
            </p>
          </div>
        </div>
      )}

      {availableModels.length === 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <div className="flex items-center space-x-2">
            <span className="text-yellow-600">⚠️</span>
            <div>
              <h4 className="font-medium text-yellow-800">No Parakeet models available</h4>
              <p className="text-sm text-yellow-700">
                Download at least one Parakeet model to enable fast transcription (up to 30x faster than Whisper).
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
