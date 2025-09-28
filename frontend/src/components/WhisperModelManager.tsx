import React, { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  ModelInfo,
  ModelStatus,
  getModelIcon,
  formatFileSize,
  getModelPerformanceBadge,
  isQuantizedModel,
  WhisperAPI
} from '../lib/whisper';
import { ModelDownloadProgress, ProgressRing, DownloadSummary } from './ModelDownloadProgress';

interface ModelManagerProps {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
  className?: string;
}

export function ModelManager({ selectedModel, onModelSelect, className = '' }: ModelManagerProps) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load persisted downloading state from localStorage
  const getPersistedDownloadingModels = () => {
    try {
      const saved = localStorage.getItem('downloading-models');
      return saved ? new Set(JSON.parse(saved)) : new Set<string>();
    } catch {
      return new Set<string>();
    }
  };

  const [downloadingModels, setDownloadingModels] = useState<Set<string>>(getPersistedDownloadingModels());

  // Persist downloading state to localStorage whenever it changes
  const updateDownloadingModels = (updater: (prev: Set<string>) => Set<string>) => {
    setDownloadingModels(prev => {
      const newSet = updater(prev);
      localStorage.setItem('downloading-models', JSON.stringify(Array.from(newSet)));
      return newSet;
    });
  };

  useEffect(() => {
    const initializeModels = async () => {
      await loadAvailableModels();
      // Check if any downloads from previous session are still in progress
      await syncDownloadStates();
    };
    initializeModels();
  }, []);

  // Check and sync download states with actual model status
  const syncDownloadStates = async () => {
    try {
      const persistedDownloading = getPersistedDownloadingModels();

      // Clean up completed downloads from persisted state
      // This handles the case where downloads completed while the app was closed
      setModels(prevModels => {
        const updatedModels = prevModels.map(model => {
          if (persistedDownloading.has(model.name) && model.status === 'Available') {
            // Download completed while app was closed - clean up localStorage
            updateDownloadingModels(prev => {
              const newSet = new Set(prev);
              newSet.delete(model.name);
              return newSet;
            });
            console.log(`Download completed while app was closed: ${model.name}`);
          }
          return model;
        });
        return updatedModels;
      });
    } catch (error) {
      console.error('Failed to sync download states:', error);
    }
  };

  // Set up download progress event listeners
  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    const setupListeners = async () => {
      console.log('Setting up download event listeners...');
      // Listen for download progress updates
      unlistenProgress = await listen<{ modelName: string; progress: number }>('model-download-progress', (event) => {
        console.log('Received model-download-progress event:', event);
        const { modelName, progress } = event.payload;
        console.log(`Download progress for ${modelName}: ${progress}%`);
        
        setModels(prevModels => prevModels.map(model => {
          if (model.name === modelName) {
            return {
              ...model,
              status: { Downloading: progress } as ModelStatus
            };
          }
          return model;
        }));
      });

      // Listen for download completion
      unlistenComplete = await listen<{ modelName: string }>('model-download-complete', (event) => {
        console.log('Received model-download-complete event:', event);
        const { modelName } = event.payload;
        console.log(`Download completed for ${modelName}`);
        
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
      unlistenError = await listen<{ modelName: string; error: string }>('model-download-error', (event) => {
        console.log('Received model-download-error event:', event);
        const { modelName, error } = event.payload;
        console.error(`Download failed for ${modelName}:`, error);
        
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
      // Cleanup listeners
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
      if (unlistenError) unlistenError();
    };
  }, []);

  const loadAvailableModels = async () => {
    try {
      setLoading(true);
      setError(null);
      
      // Initialize Whisper engine if not already done
      await WhisperAPI.init();
      
      // Get actual model list from whisper-rs backend
      const modelList = await WhisperAPI.getAvailableModels();
      console.log(modelList)

      // Immediately apply persisted downloading states before setting models
      const persistedDownloading = getPersistedDownloadingModels();
      const modelsWithDownloadState = modelList.map(model => {
        if (persistedDownloading.has(model.name) && model.status !== 'Available') {
          // Model is in localStorage as downloading and not yet available - show as downloading
          return {
            ...model,
            status: { Downloading: 0 } as ModelStatus
          };
        }
        return model;
      });

      setModels(modelsWithDownloadState);
      
      // Validate current selection and auto-select if needed
      if (selectedModel) {
        // Check if the currently selected model is actually available
        const currentModel = modelsWithDownloadState.find(m => m.name === selectedModel);
        if (!currentModel || currentModel.status !== 'Available') {
          // Clear invalid selection
          if (onModelSelect) {
            onModelSelect('');
          }
        }
      }

      // Auto-select best available model if none selected or selection was cleared
      if (!selectedModel || !modelsWithDownloadState.find(m => m.name === selectedModel && m.status === 'Available')) {
        const availableModel = modelsWithDownloadState.find(m => m.status === 'Available');
        if (availableModel && onModelSelect) {
          onModelSelect(availableModel.name);
        }
      }
    } catch (err) {
      console.error('Failed to load models:', err);
      setError(err instanceof Error ? err.message : 'Failed to load models');
    } finally {
      setLoading(false);
    }
  };

  const downloadModel = async (modelName: string) => {
    // Prevent multiple downloads of the same model
    if (downloadingModels.has(modelName)) {
      console.log(`Download already in progress for model: ${modelName}`);
      return;
    }

    try {
      console.log(`Starting download for model: ${modelName}`);
      updateDownloadingModels(prev => new Set([...prev, modelName]));

      // Immediately set status to downloading with 0% progress
      setModels(prevModels => prevModels.map(model => {
        if (model.name === modelName) {
          return {
            ...model,
            status: { Downloading: 0 } as ModelStatus
          };
        }
        return model;
      }));

      // Start real download using WhisperAPI
      console.log(`Calling WhisperAPI.downloadModel for: ${modelName}`);
      await WhisperAPI.downloadModel(modelName);
      console.log(`Download completed for: ${modelName}`);

      // Wait a moment for file to be written completely
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Refresh model list to get updated status
      console.log(`Refreshing model list after download of: ${modelName}`);
      await loadAvailableModels();

      // Verify the model was actually downloaded
      const updatedModels = await WhisperAPI.getAvailableModels();
      const downloadedModel = updatedModels.find(m => m.name === modelName);

      if (downloadedModel?.status !== 'Available') {
        throw new Error(`Model download verification failed. Model status: ${JSON.stringify(downloadedModel?.status)}`);
      }

      updateDownloadingModels(prev => {
        const newSet = new Set(prev);
        newSet.delete(modelName);
        return newSet;
      });

      console.log(`Successfully downloaded and verified model: ${modelName}`);

      // Auto-select the downloaded model only if verification passed
      if (onModelSelect) {
        onModelSelect(modelName);
      }
    } catch (err) {
      console.error('Failed to download model:', err);

      // Show detailed error message
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

      // Show user-friendly error notification
      console.error(`Model download failed for ${modelName}: ${errorMessage}`);
    }
  };

  const selectModel = async (modelName: string) => {
    if (onModelSelect) {
      onModelSelect(modelName);
    }
  };

  const deleteCorruptedModel = async (modelName: string) => {
    try {
      console.log(`Attempting to delete corrupted model: ${modelName}`);

      // Show confirmation dialog
      const confirmed = window.confirm(
        `Are you sure you want to delete the corrupted file for "${modelName}"? This cannot be undone.`
      );

      if (!confirmed) {
        return;
      }

      const result = await WhisperAPI.deleteCorruptedModel(modelName);
      console.log(`Delete result: ${result}`);

      // Refresh model list to update status
      await loadAvailableModels();

      console.log(`Successfully deleted corrupted model: ${modelName}`);
    } catch (err) {
      console.error('Failed to delete corrupted model:', err);

      // Show user-friendly error notification
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
          <div className="w-2 h-2 bg-orange-500 rounded-full"></div>
          <span className="text-xs text-orange-700" title={`File corrupted: ${fileSizeMB}MB (expected ≥${expectedSizeMB}MB)`}>
            Corrupted
          </span>
        </div>
      );
    }
    return null;
  };

  const availableModels = models.filter(m => m.status === 'Available');
  const totalSizeMb = availableModels.reduce((sum, m) => sum + m.size_mb, 0);

  if (loading) {
    return (
      <div className={`animate-pulse space-y-4 ${className}`}>
        <div className="h-4 bg-gray-200 rounded w-3/4"></div>
        <div className="space-y-3">
          {[1, 2, 3].map(i => (
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
            <h4 className="font-medium text-red-800">Failed to load models</h4>
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
      

      <div className="grid gap-4 max-h-[calc(100vh-450px)] overflow-y-auto">
        {models.map((model) => {
          const isSelected = selectedModel === model.name;
          const isDownloading = typeof model.status === 'object' && 'Downloading' in model.status;
          const isAvailable = model.status === 'Available';
          console.log(model.size_mb)
          
          return (
            <div key={model.name} className="space-y-2 ">
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
                          <span>Whisper {model.name}</span>
                          {isSelected && (
                            <span className="bg-blue-600 text-white px-2 py-1 rounded-full text-xs">
                              Active
                            </span>
                          )}
                          {isQuantizedModel(model.name) && (
                            <span className={`px-2 py-1 rounded-full text-xs ${
                              getModelPerformanceBadge(model.name).color === 'green'
                                ? 'bg-green-100 text-green-700'
                                : getModelPerformanceBadge(model.name).color === 'orange'
                                ? 'bg-orange-100 text-orange-700'
                                : 'bg-gray-100 text-gray-700'
                            }`}>
                              {getModelPerformanceBadge(model.name).label}
                            </span>
                          )}
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

                    {typeof model.status === 'object' && 'Corrupted' in model.status && (
                      <div className="flex flex-col space-y-1">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteCorruptedModel(model.name);
                          }}
                          className="bg-orange-600 text-white px-3 py-1 rounded text-xs hover:bg-orange-700 transition-colors"
                        >
                          Delete
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            downloadModel(model.name);
                          }}
                          className="bg-blue-600 text-white px-3 py-1 rounded text-xs hover:bg-blue-700 transition-colors"
                        >
                          Re-download
                        </button>
                      </div>
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
              Using <strong>{selectedModel}</strong> model for transcription
            </p>
          </div>
        </div>
      )}

      {availableModels.length === 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <div className="flex items-center space-x-2">
            <span className="text-yellow-600">⚠️</span>
            <div>
              <h4 className="font-medium text-yellow-800">No models available</h4>
              <p className="text-sm text-yellow-700">
                Download at least one Whisper model to enable local transcription.
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
