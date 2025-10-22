import React, { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { toast } from 'sonner';
import {
  ParakeetModelInfo,
  ModelStatus,
  ParakeetAPI,
  getModelDisplayInfo,
  getModelDisplayName,
  formatFileSize
} from '../lib/parakeet';

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
  const [downloadingModels, setDownloadingModels] = useState<Set<string>>(new Set());

  // Refs for stable callbacks
  const onModelSelectRef = useRef(onModelSelect);
  const autoSaveRef = useRef(autoSave);

  // Progress throttle map to prevent rapid updates
  const progressThrottleRef = useRef<Map<string, { progress: number; timestamp: number }>>(new Map());

  // Update refs when props change
  useEffect(() => {
    onModelSelectRef.current = onModelSelect;
    autoSaveRef.current = autoSave;
  }, [onModelSelect, autoSave]);

  // Initialize and load models
  useEffect(() => {
    if (initialized) return;

    const initializeModels = async () => {
      try {
        setLoading(true);
        await ParakeetAPI.init();
        const modelList = await ParakeetAPI.getAvailableModels();
        setModels(modelList);

        // Auto-select first available model if none selected
        if (!selectedModel) {
          const recommendedModel = modelList.find(m =>
            m.name === 'parakeet-tdt-0.6b-v3-int8' && m.status === 'Available'
          );
          const anyAvailable = modelList.find(m => m.status === 'Available');
          const toSelect = recommendedModel || anyAvailable;

          if (toSelect && onModelSelect) {
            onModelSelect(toSelect.name);
          }
        }

        setInitialized(true);
      } catch (err) {
        console.error('Failed to initialize Parakeet:', err);
        setError(err instanceof Error ? err.message : 'Failed to load models');
        toast.error('Failed to load transcription models', {
          description: err instanceof Error ? err.message : 'Unknown error',
          duration: 5000
        });
      } finally {
        setLoading(false);
      }
    };

    initializeModels();
  }, [initialized, selectedModel, onModelSelect]);

  // Set up event listeners for download progress
  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    const setupListeners = async () => {
      console.log('[ParakeetModelManager] Setting up event listeners...');

      // Download progress with throttling
      unlistenProgress = await listen<{ modelName: string; progress: number }>(
        'parakeet-model-download-progress',
        (event) => {
          const { modelName, progress } = event.payload;
          const now = Date.now();
          const throttleData = progressThrottleRef.current.get(modelName);

          // Throttle: only update if 300ms passed OR progress jumped by 5%+
          const shouldUpdate = !throttleData ||
            now - throttleData.timestamp > 300 ||
            Math.abs(progress - throttleData.progress) >= 5;

          if (shouldUpdate) {
            console.log(`[ParakeetModelManager] Progress update for ${modelName}: ${progress}%`);
            progressThrottleRef.current.set(modelName, { progress, timestamp: now });

            setModels(prevModels =>
              prevModels.map(model =>
                model.name === modelName
                  ? { ...model, status: { Downloading: progress } as ModelStatus }
                  : model
              )
            );
          }
        }
      );

      // Download complete
      unlistenComplete = await listen<{ modelName: string }>(
        'parakeet-model-download-complete',
        (event) => {
          const { modelName } = event.payload;
          const displayInfo = getModelDisplayInfo(modelName);
          const displayName = displayInfo?.friendlyName || modelName;

          setModels(prevModels =>
            prevModels.map(model =>
              model.name === modelName
                ? { ...model, status: 'Available' as ModelStatus }
                : model
            )
          );

          setDownloadingModels(prev => {
            const newSet = new Set(prev);
            newSet.delete(modelName);
            return newSet;
          });

          // Clean up throttle data
          progressThrottleRef.current.delete(modelName);

          toast.success(`${displayInfo?.icon || '✓'} ${displayName} ready!`, {
            description: 'Model downloaded and ready to use',
            duration: 4000
          });

          // Auto-select after download using stable refs
          if (onModelSelectRef.current) {
            onModelSelectRef.current(modelName);
            if (autoSaveRef.current) {
              saveModelSelection(modelName);
            }
          }
        }
      );

      // Download error
      unlistenError = await listen<{ modelName: string; error: string }>(
        'parakeet-model-download-error',
        (event) => {
          const { modelName, error } = event.payload;
          const displayInfo = getModelDisplayInfo(modelName);
          const displayName = displayInfo?.friendlyName || modelName;

          setModels(prevModels =>
            prevModels.map(model =>
              model.name === modelName
                ? { ...model, status: { Error: error } as ModelStatus }
                : model
            )
          );

          setDownloadingModels(prev => {
            const newSet = new Set(prev);
            newSet.delete(modelName);
            return newSet;
          });

          // Clean up throttle data
          progressThrottleRef.current.delete(modelName);

          toast.error(`Failed to download ${displayName}`, {
            description: error,
            duration: 6000,
            action: {
              label: 'Retry',
              onClick: () => downloadModel(modelName)
            }
          });
        }
      );
    };

    setupListeners();

    return () => {
      console.log('[ParakeetModelManager] Cleaning up event listeners...');
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
      if (unlistenError) unlistenError();
    };
  }, []); // Empty dependency array - listeners use refs for stable callbacks

  const saveModelSelection = async (modelName: string) => {
    try {
      await invoke('api_save_transcript_config', {
        provider: 'parakeet',
        model: modelName,
        apiKey: null
      });
    } catch (error) {
      console.error('Failed to save model selection:', error);
    }
  };

  const cancelDownload = async (modelName: string) => {
    const displayInfo = getModelDisplayInfo(modelName);
    const displayName = displayInfo?.friendlyName || modelName;

    try {
      await ParakeetAPI.cancelDownload(modelName);

      setDownloadingModels(prev => {
        const newSet = new Set(prev);
        newSet.delete(modelName);
        return newSet;
      });

      setModels(prevModels =>
        prevModels.map(model =>
          model.name === modelName
            ? { ...model, status: 'Missing' as ModelStatus }
            : model
        )
      );

      // Clean up throttle data
      progressThrottleRef.current.delete(modelName);

      toast.info(`${displayName} download cancelled`, {
        duration: 3000
      });
    } catch (err) {
      console.error('Failed to cancel download:', err);
      toast.error('Failed to cancel download', {
        description: err instanceof Error ? err.message : 'Unknown error',
        duration: 4000
      });
    }
  };

  const downloadModel = async (modelName: string) => {
    if (downloadingModels.has(modelName)) return;

    const displayInfo = getModelDisplayInfo(modelName);
    const displayName = displayInfo?.friendlyName || modelName;

    try {
      setDownloadingModels(prev => new Set([...prev, modelName]));

      setModels(prevModels =>
        prevModels.map(model =>
          model.name === modelName
            ? { ...model, status: { Downloading: 0 } as ModelStatus }
            : model
        )
      );

      toast.info(`Downloading ${displayName}...`, {
        description: 'This may take a few minutes',
        duration: 5000  // Auto-dismiss after 5 seconds
      });

      await ParakeetAPI.downloadModel(modelName);
    } catch (err) {
      console.error('Download failed:', err);
      setDownloadingModels(prev => {
        const newSet = new Set(prev);
        newSet.delete(modelName);
        return newSet;
      });

      const errorMessage = err instanceof Error ? err.message : 'Download failed';
      setModels(prev =>
        prev.map(model =>
          model.name === modelName ? { ...model, status: { Error: errorMessage } } : model
        )
      );
    }
  };

  const selectModel = async (modelName: string) => {
    if (onModelSelect) {
      onModelSelect(modelName);
    }

    if (autoSave) {
      await saveModelSelection(modelName);
    }

    const displayInfo = getModelDisplayInfo(modelName);
    const displayName = displayInfo?.friendlyName || modelName;
    toast.success(`Switched to ${displayName}`, {
      duration: 3000
    });
  };

  const deleteModel = async (modelName: string) => {
    const displayInfo = getModelDisplayInfo(modelName);
    const displayName = displayInfo?.friendlyName || modelName;

    try {
      await ParakeetAPI.deleteCorruptedModel(modelName);

      // Refresh models list
      const modelList = await ParakeetAPI.getAvailableModels();
      setModels(modelList);

      toast.success(`${displayName} deleted`, {
        description: 'Model removed to free up space',
        duration: 3000
      });

      // If deleted model was selected, clear selection
      if (selectedModel === modelName && onModelSelect) {
        onModelSelect('');
      }
    } catch (err) {
      console.error('Failed to delete model:', err);
      toast.error(`Failed to delete ${displayName}`, {
        description: err instanceof Error ? err.message : 'Delete failed',
        duration: 4000
      });
    }
  };

  if (loading) {
    return (
      <div className={`space-y-3 ${className}`}>
        <div className="animate-pulse space-y-3">
          <div className="h-20 bg-gray-100 rounded-lg"></div>
          <div className="h-20 bg-gray-100 rounded-lg"></div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={`bg-red-50 border border-red-200 rounded-lg p-4 ${className}`}>
        <p className="text-sm text-red-800">Failed to load models</p>
        <p className="text-xs text-red-600 mt-1">{error}</p>
      </div>
    );
  }

  const recommendedModel = models.find(m =>
    m.name === 'parakeet-tdt-0.6b-v3-int8'
  );
  const otherModels = models.filter(m =>
    m.name !== 'parakeet-tdt-0.6b-v3-int8'
  );

  return (
    <div className={`space-y-3 ${className}`}>
      {/* Recommended Model */}
      {recommendedModel && (
        <ModelCard
          model={recommendedModel}
          isSelected={selectedModel === recommendedModel.name}
          isRecommended={true}
          onSelect={() => {
            if (recommendedModel.status === 'Available') {
              selectModel(recommendedModel.name);
            }
          }}
          onDownload={() => downloadModel(recommendedModel.name)}
          onCancel={() => cancelDownload(recommendedModel.name)}
          onDelete={() => deleteModel(recommendedModel.name)}
          isDownloading={downloadingModels.has(recommendedModel.name)}
        />
      )}

      {/* Other Models */}
      {otherModels.length > 0 && (
        <div className="space-y-3">
          {otherModels.map(model => (
            <ModelCard
              key={model.name}
              model={model}
              isSelected={selectedModel === model.name}
              isRecommended={false}
              onSelect={() => {
                if (model.status === 'Available') {
                  selectModel(model.name);
                }
              }}
              onDownload={() => downloadModel(model.name)}
              onCancel={() => cancelDownload(model.name)}
              onDelete={() => deleteModel(model.name)}
              isDownloading={downloadingModels.has(model.name)}
            />
          ))}
        </div>
      )}

      {/* Helper text */}
      {selectedModel && (
        <motion.div
          initial={{ opacity: 0, y: -5 }}
          animate={{ opacity: 1, y: 0 }}
          className="text-xs text-gray-500 text-center pt-2"
        >
          Using {getModelDisplayName(selectedModel)} for transcription
        </motion.div>
      )}
    </div>
  );
}

// Model Card Component
interface ModelCardProps {
  model: ParakeetModelInfo;
  isSelected: boolean;
  isRecommended: boolean;
  onSelect: () => void;
  onDownload: () => void;
  onCancel: () => void;
  onDelete: () => void;
  isDownloading: boolean;
}

function ModelCard({
  model,
  isSelected,
  isRecommended,
  onSelect,
  onDownload,
  onCancel,
  onDelete,
  isDownloading
}: ModelCardProps) {
  const [isHovered, setIsHovered] = useState(false);
  const displayInfo = getModelDisplayInfo(model.name);
  const displayName = displayInfo?.friendlyName || model.name;
  const icon = displayInfo?.icon || '📦';
  const tagline = displayInfo?.tagline || model.description || '';

  const isAvailable = model.status === 'Available';
  const isMissing = model.status === 'Missing';
  const isError = typeof model.status === 'object' && 'Error' in model.status;
  const isCorrupted = typeof model.status === 'object' && 'Corrupted' in model.status;
  const downloadProgress =
    typeof model.status === 'object' && 'Downloading' in model.status
      ? model.status.Downloading
      : null;

  return (
    <motion.div
      initial={{ opacity: 0, y: 5 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      className={`
        relative rounded-lg border-2 transition-all cursor-pointer
        ${isSelected && isAvailable
          ? 'border-blue-500 bg-blue-50'
          : isAvailable
            ? 'border-gray-200 hover:border-gray-300 bg-white'
            : 'border-gray-200 bg-gray-50'
        }
        ${isAvailable ? '' : 'cursor-default'}
      `}
      onClick={() => {
        if (isAvailable) onSelect();
      }}
    >
      {/* Recommended Badge */}
      {isRecommended && (
        <div className="absolute -top-2 -right-2 bg-blue-600 text-white text-xs px-2 py-0.5 rounded-full font-medium">
          Recommended
        </div>
      )}

      <div className="p-4">
        <div className="flex items-start justify-between mb-3">
          <div className="flex-1">
            {/* Model Name */}
            <div className="flex items-center gap-2 mb-1">
              <span className="text-2xl">{icon}</span>
              <h3 className="font-semibold text-gray-900">{displayName}</h3>
              {isSelected && isAvailable && (
                <motion.span
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  className="bg-blue-600 text-white px-2 py-0.5 rounded-full text-xs font-medium flex items-center gap-1"
                >
                  ✓
                </motion.span>
              )}
            </div>

            {/* Tagline */}
            <p className="text-sm text-gray-600 ml-9">{tagline}</p>
          </div>

          {/* Status/Action */}
          <div className="ml-4 flex items-center gap-2">
            {isAvailable && (
              <>
                <div className="flex items-center gap-1.5 text-green-600">
                  <div className="w-2 h-2 bg-green-500 rounded-full"></div>
                  <span className="text-xs font-medium">Ready</span>
                </div>
                <AnimatePresence>
                  {isHovered && (
                    <motion.button
                      initial={{ opacity: 0, scale: 0.8 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.8 }}
                      transition={{ duration: 0.15 }}
                      onClick={(e) => {
                        e.stopPropagation();
                        onDelete();
                      }}
                      className="text-gray-400 hover:text-red-600 transition-colors p-1"
                      title="Delete model to free up space"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </motion.button>
                  )}
                </AnimatePresence>
              </>
            )}

            {isMissing && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onDownload();
                }}
                className="bg-blue-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-blue-700 transition-colors"
              >
                Download
              </button>
            )}

            {downloadProgress === null && isError && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onDownload();
                }}
                className="bg-red-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-red-700 transition-colors"
              >
                Retry
              </button>
            )}

            {isCorrupted && (
              <div className="flex gap-2">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete();
                  }}
                  className="bg-orange-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-orange-700 transition-colors"
                >
                  Delete
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDownload();
                  }}
                  className="bg-blue-600 text-white px-3 py-1.5 rounded-md text-sm font-medium hover:bg-blue-700 transition-colors"
                >
                  Re-download
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Full-width Download Progress Bar - PROMINENT */}
        {downloadProgress !== null && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="mt-3 pt-3 border-t border-gray-200"
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-blue-600">Downloading...</span>
                <span className="text-sm font-semibold text-blue-600">{Math.round(downloadProgress)}%</span>
              </div>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onCancel();
                }}
                className="text-xs text-gray-600 hover:text-red-600 font-medium transition-colors px-2 py-1 rounded hover:bg-red-50"
                title="Cancel download"
              >
                Cancel
              </button>
            </div>
            <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
              <motion.div
                className="h-full bg-gradient-to-r from-blue-500 to-blue-600 rounded-full"
                initial={{ width: 0 }}
                animate={{ width: `${downloadProgress}%` }}
                transition={{ duration: 0.3, ease: 'easeOut' }}
              />
            </div>
            <p className="text-xs text-gray-500 mt-1">
              {model.size_mb ? (
                <>
                  {formatFileSize(model.size_mb * downloadProgress / 100)} / {formatFileSize(model.size_mb)}
                </>
              ) : (
                'Downloading...'
              )}
            </p>
          </motion.div>
        )}
      </div>
    </motion.div>
  );
}
