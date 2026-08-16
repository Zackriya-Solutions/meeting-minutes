import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import Analytics from '@/lib/analytics';
import { applyPinnedSummaryLanguageToMeeting } from '@/lib/summary-language-preferences';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { loadBetaFeatures } from '@/types/betaFeatures';

export interface AudioFileInfo {
  path: string;
  filename: string;
  duration_seconds: number;
  size_bytes: number;
  format: string;
  existing_meeting: ExistingAudioMeeting | null;
  duplicate_source_path: string | null;
}

export interface ExistingAudioMeeting {
  meeting_id: string;
  title: string;
  created_at: string;
  /** How that import was processed. `null` means it predates the record: unknown, not "no". */
  denoise_applied: boolean | null;
}

export interface ImportProgress {
  stage: string;
  progress_percentage: number;
  message: string;
}

export interface ImportResult {
  meeting_id: string;
  title: string;
  segments_count: number;
  duration_seconds: number;
  processable_segments: number;
  transcribed_segments: number;
  empty_segments: number;
  transcription_coverage: number | null;
  average_confidence: number | null;
}

export interface ImportError {
  error: string;
}

export interface BatchImportItem {
  source_path: string;
  title: string;
}

export interface BatchImportProgress {
  current_index: number;
  total: number;
  current_title: string;
  completed: number;
  skipped: number;
  truncated: number;
  failed: number;
  state: string;
}

export interface BatchImportResult {
  total: number;
  imported: ImportResult[];
  skipped: Array<BatchImportItem & { existing_meeting: ExistingAudioMeeting }>;
  truncated: BatchImportItem[];
  failed: Array<BatchImportItem & { error: string }>;
  cancelled: boolean;
}

export type ImportStatus = 'idle' | 'validating' | 'processing' | 'complete' | 'error';

export interface UseImportAudioOptions {
  onComplete?: (result: ImportResult) => void;
  onDuplicate?: (existing: ExistingAudioMeeting) => void;
  onBatchComplete?: (result: BatchImportResult) => void;
  onError?: (error: string) => void;
}

export interface UseImportAudioReturn {
  status: ImportStatus;
  fileInfo: AudioFileInfo | null;
  batchFiles: AudioFileInfo[];
  progress: ImportProgress | null;
  batchProgress: BatchImportProgress | null;
  error: string | null;
  isProcessing: boolean;
  isBusy: boolean;
  selectFile: () => Promise<AudioFileInfo | null>;
  selectFolder: () => Promise<AudioFileInfo[]>;
  validateFile: (path: string) => Promise<AudioFileInfo | null>;
  startImport: (
    sourcePath: string,
    title: string,
    language?: string | null,
    model?: string | null,
    provider?: string | null,
    forceReimport?: boolean
  ) => Promise<void>;
  startBatchImport: (
    items: BatchImportItem[],
    language?: string | null,
    model?: string | null,
    provider?: string | null
  ) => Promise<void>;
  cancelImport: () => Promise<void>;
  reset: () => void;
}

export function useImportAudio({
  onComplete,
  onDuplicate,
  onBatchComplete,
  onError,
}: UseImportAudioOptions = {}): UseImportAudioReturn {
  const t = useT();
  const [status, setStatus] = useState<ImportStatus>('idle');
  const [fileInfo, setFileInfo] = useState<AudioFileInfo | null>(null);
  const [batchFiles, setBatchFiles] = useState<AudioFileInfo[]>([]);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [batchProgress, setBatchProgress] = useState<BatchImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Stable refs for callbacks to avoid listener re-registration on every render
  const onCompleteRef = useRef(onComplete);
  const onDuplicateRef = useRef(onDuplicate);
  const onBatchCompleteRef = useRef(onBatchComplete);
  const onErrorRef = useRef(onError);
  useEffect(() => { onCompleteRef.current = onComplete; }, [onComplete]);
  useEffect(() => { onDuplicateRef.current = onDuplicate; }, [onDuplicate]);
  useEffect(() => { onBatchCompleteRef.current = onBatchComplete; }, [onBatchComplete]);
  useEffect(() => { onErrorRef.current = onError; }, [onError]);

  // Cancellation guard: prevents late events from updating state after cancel
  const isCancelledRef = useRef(false);

  // Set up event listeners (registered once, use refs for callbacks)
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const cleanedUpRef = { current: false };

    const setupListeners = async () => {
      // Progress events
      const unlistenProgress = await listen<ImportProgress>(
        'import-progress',
        (event) => {
          if (isCancelledRef.current) return;
          setProgress(event.payload);
          setStatus('processing');
        }
      );
      if (cleanedUpRef.current) {
        unlistenProgress();
        return;
      }
      unlisteners.push(unlistenProgress);

      // Completion event
      const unlistenComplete = await listen<ImportResult>(
        'import-complete',
        async (event) => {
          if (isCancelledRef.current) return;

          await Analytics.track('import_audio_completed', {
            success: 'true',
            duration_seconds: event.payload.duration_seconds.toString(),
            segments_count: event.payload.segments_count.toString()
          });

          setStatus('complete');
          setProgress(null);
          try {
            await applyPinnedSummaryLanguageToMeeting(event.payload.meeting_id);
          } catch (error) {
            console.warn('Failed to apply pinned summary language to imported meeting:', error);
            toast.warning(t('Could not apply default summary language'), {
              description: t('The imported meeting was saved, but the default summary language was not applied.'),
            });
          }
          onCompleteRef.current?.(event.payload);
        }
      );
      if (cleanedUpRef.current) {
        unlistenComplete();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenComplete);

      const unlistenDuplicate = await listen<ExistingAudioMeeting>(
        'import-duplicate',
        (event) => {
          if (isCancelledRef.current) return;
          setStatus('complete');
          setProgress(null);
          onDuplicateRef.current?.(event.payload);
        }
      );
      if (cleanedUpRef.current) {
        unlistenDuplicate();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenDuplicate);

      const unlistenBatchProgress = await listen<BatchImportProgress>(
        'batch-import-progress',
        (event) => {
          if (isCancelledRef.current) return;
          setBatchProgress(event.payload);
          setStatus('processing');
        }
      );
      if (cleanedUpRef.current) {
        unlistenBatchProgress();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenBatchProgress);

      const unlistenBatchComplete = await listen<BatchImportResult>(
        'batch-import-complete',
        async (event) => {
          if (isCancelledRef.current) return;

          setStatus('complete');
          setProgress(null);
          setBatchProgress(null);
          for (const imported of event.payload.imported) {
            try {
              await applyPinnedSummaryLanguageToMeeting(imported.meeting_id);
            } catch (error) {
              console.warn('Failed to apply pinned summary language to batch-imported meeting:', error);
            }
          }
          onBatchCompleteRef.current?.(event.payload);
        }
      );
      if (cleanedUpRef.current) {
        unlistenBatchComplete();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenBatchComplete);

      // Error event
      const unlistenError = await listen<ImportError>(
        'import-error',
        async (event) => {
          if (isCancelledRef.current) return;

          await Analytics.trackError('import_audio_failed', event.payload.error);

          setStatus('error');
          setError(event.payload.error);
          onErrorRef.current?.(event.payload.error);
        }
      );
      if (cleanedUpRef.current) {
        unlistenError();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenError);

      const unlistenBatchError = await listen<ImportError>(
        'batch-import-error',
        async (event) => {
          if (isCancelledRef.current) return;
          setStatus('error');
          setError(event.payload.error);
          onErrorRef.current?.(event.payload.error);
        }
      );
      if (cleanedUpRef.current) {
        unlistenBatchError();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenBatchError);
    };

    setupListeners();

    return () => {
      cleanedUpRef.current = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  // Select file using native file dialog
  const selectFile = useCallback(async (): Promise<AudioFileInfo | null> => {
    setStatus('validating');
    setError(null);

    try {
      const result = await invoke<AudioFileInfo | null>('select_and_validate_audio_command');
      if (result) {
        setFileInfo(result);
        setBatchFiles([]);
        setStatus('idle');
        return result;
      } else {
        // User cancelled
        setStatus('idle');
        return null;
      }
    } catch (err: any) {
      setStatus('error');
      const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || t('Failed to validate file'));
      setError(errorMsg);
      onErrorRef.current?.(errorMsg);
      return null;
    }
  }, []);

  const selectFolder = useCallback(async (): Promise<AudioFileInfo[]> => {
    setStatus('validating');
    setError(null);

    try {
      const result = await invoke<AudioFileInfo[] | null>('select_and_validate_audio_folder_command');
      if (!result) {
        setStatus('idle');
        return [];
      }
      setBatchFiles(result);
      setFileInfo(null);
      setStatus('idle');
      return result;
    } catch (err: any) {
      setStatus('error');
      const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || t('Failed to validate folder'));
      setError(errorMsg);
      onErrorRef.current?.(errorMsg);
      return [];
    }
  }, [t]);

  // Validate a file from a given path (for drag-drop)
  const validateFile = useCallback(async (path: string): Promise<AudioFileInfo | null> => {
    setStatus('validating');
    setError(null);

    try {
      const result = await invoke<AudioFileInfo>('validate_audio_file_command', { path });
      setFileInfo(result);
      setStatus('idle');
      return result;
    } catch (err: any) {
      setStatus('error');
      const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || t('Failed to validate file'));
      setError(errorMsg);
      onErrorRef.current?.(errorMsg);
      return null;
    }
  }, []);

  // Start the import process
  const startImport = useCallback(
    async (
      sourcePath: string,
      title: string,
      language?: string | null,
      model?: string | null,
      provider?: string | null,
      forceReimport?: boolean
    ) => {
      isCancelledRef.current = false;
      setStatus('processing');
      setError(null);
      setProgress(null);

      try {
        if (fileInfo) {
          await Analytics.track('import_audio_started', {
            file_size_bytes: fileInfo.size_bytes.toString(),
            duration_seconds: fileInfo.duration_seconds.toString(),
            language: language || 'auto',
            model_provider: provider || '',
            model_name: model || ''
          });
        }

        await invoke('start_import_audio_command', {
          sourcePath,
          title,
          language: language || null,
          model: model || null,
          provider: provider || null,
          denoiseAudio: loadBetaFeatures().noisyAudioDenoising,
          forceReimport: forceReimport ?? false,
        });
      } catch (err: any) {
        setStatus('error');
        const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || t('Failed to start import'));
        setError(errorMsg);

        await Analytics.trackError('import_audio_failed', errorMsg);

        onErrorRef.current?.(errorMsg);
      }
    },
    [fileInfo]
  );

  const startBatchImport = useCallback(
    async (
      items: BatchImportItem[],
      language?: string | null,
      model?: string | null,
      provider?: string | null
    ) => {
      isCancelledRef.current = false;
      setStatus('processing');
      setError(null);
      setProgress(null);
      setBatchProgress(null);

      try {
        await invoke('start_batch_import_audio_command', {
          items,
          language: language || null,
          model: model || null,
          provider: provider || null,
          denoiseAudio: loadBetaFeatures().noisyAudioDenoising,
        });
      } catch (err: any) {
        setStatus('error');
        const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || t('Failed to start batch import'));
        setError(errorMsg);
        onErrorRef.current?.(errorMsg);
      }
    },
    [t]
  );

  // Cancel ongoing import
  const cancelImport = useCallback(async () => {
    isCancelledRef.current = true;
    try {
      await invoke('cancel_import_command');
      setStatus('idle');
      setProgress(null);
      setBatchProgress(null);
    } catch (err: any) {
      console.error('Failed to cancel import:', err);
    }
  }, []);

  // Reset all state
  const reset = useCallback(() => {
    isCancelledRef.current = false;
    setStatus('idle');
    setFileInfo(null);
    setBatchFiles([]);
    setProgress(null);
    setBatchProgress(null);
    setError(null);
  }, []);

  return {
    status,
    fileInfo,
    batchFiles,
    progress,
    batchProgress,
    error,
    isProcessing: status === 'processing',
    isBusy: status === 'processing' || status === 'validating',
    selectFile,
    selectFolder,
    validateFile,
    startImport,
    startBatchImport,
    cancelImport,
    reset,
  };
}
