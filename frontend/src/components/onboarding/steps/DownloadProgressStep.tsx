import React, { useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Mic, Sparkles, Users, Check, Loader2, Download, RefreshCw } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { toast } from 'sonner';
import { motion, AnimatePresence } from 'framer-motion';
import { getSummaryModelSizeLabel, getSummaryModelSizeMb } from '@/lib/onboarding-summary-model';
import { useT } from '@/lib/i18n';

type DownloadStatus = 'waiting' | 'downloading' | 'completed' | 'error';

/** Shown until `gigaam_status` reports the selected variant's real size. */
const TRANSCRIPTION_FALLBACK_MB = 987;
/** Shown until `diarization_status` reports the real combined size. */
const DIARIZATION_FALLBACK_MB = 34;

interface DownloadState {
  status: DownloadStatus;
  progress: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
  error?: string;
}

export function DownloadProgressStep() {
  const t = useT();
  const {
    goNext,
    selectedSummaryModel,
    recommendedSummaryModel,
    transcriptionModelDownloaded,
    setTranscriptionModelDownloaded,
    summaryModelDownloaded,
    setSummaryModelDownloaded,
    startBackgroundDownloads,
    retryTranscriptionModelDownload,
    completeOnboarding,
  } = useOnboarding();

  const [isMac, setIsMac] = useState(false);
  /** Label and size of the transcription variant a fresh install will fetch. */
  const [transcriptionModel, setTranscriptionModel] = useState<{ label: string; sizeMb: number }>({
    label: 'GigaAM v3',
    sizeMb: TRANSCRIPTION_FALLBACK_MB,
  });

  const [transcriptionState, setTranscriptionState] = useState<DownloadState>({
    status: transcriptionModelDownloaded ? 'completed' : 'waiting',
    progress: transcriptionModelDownloaded ? 100 : 0,
    downloadedMb: 0,
    totalMb: TRANSCRIPTION_FALLBACK_MB,
    speedMbps: 0,
  });

  const [summaryState, setSummaryState] = useState<DownloadState>({
    status: summaryModelDownloaded ? 'completed' : 'waiting',
    progress: summaryModelDownloaded ? 100 : 0,
    downloadedMb: 0,
    totalMb: 0,
    speedMbps: 0,
  });

  // Speaker recognition. Kept as local state rather than onboarding context: unlike the
  // transcription and summary models, nothing outside this step gates on it, and its
  // presence is always re-derivable from `diarization_status`.
  const [diarizationState, setDiarizationState] = useState<DownloadState>({
    status: 'waiting',
    progress: 0,
    downloadedMb: 0,
    totalMb: DIARIZATION_FALLBACK_MB,
    speedMbps: 0,
  });

  // Any card still transferring — drives the background-downloads banner and the
  // Continue button's wording.
  const anyDownloading =
    transcriptionState.status === 'downloading' ||
    summaryState.status === 'downloading' ||
    diarizationState.status === 'downloading';

  const [isCompleting, setIsCompleting] = useState(false);
  const transcriptionDownloadStartedRef = useRef(false);
  const summaryDownloadStartedRef = useRef(false);
  const diarizationDownloadStartedRef = useRef(false);
  const retryingRef = useRef(false);
  const retryingSummaryRef = useRef(false);
  const retryingDiarizationRef = useRef(false);

  // Retry download handler
  const handleRetryDownload = async () => {
    // Prevent multiple simultaneous retries
    if (retryingRef.current) {
      console.log('[DownloadProgressStep] Retry already in progress, ignoring');
      return;
    }

    console.log('[DownloadProgressStep] Retrying the transcription model download');
    retryingRef.current = true;

    // Reset error state
    setTranscriptionState((prev) => ({
      ...prev,
      status: 'downloading',
      error: undefined,
      progress: 0,
      downloadedMb: 0,
      speedMbps: 0,
    }));

    try {
      await retryTranscriptionModelDownload();
      // Progress events will update state
    } catch (error) {
      console.error('[DownloadProgressStep] Retry failed:', error);
      setTranscriptionState((prev) => ({
        ...prev,
        status: 'error',
        error: error instanceof Error ? error.message : String(error),
      }));

      toast.error(t('Download retry failed'), {
        description: t('Please check your connection and try again.'),
      });
    } finally {
      // Allow retry again after 2 seconds
      setTimeout(() => {
        retryingRef.current = false;
      }, 2000);
    }
  };

  // Retry summary download handler
  const handleRetrySummaryDownload = async () => {
    // Prevent multiple simultaneous retries
    if (retryingSummaryRef.current) {
      console.log('[DownloadProgressStep] Summary retry already in progress, ignoring');
      return;
    }

    console.log('[DownloadProgressStep] Retrying summary model download');
    retryingSummaryRef.current = true;

    // Reset error state
    setSummaryState((prev) => ({
      ...prev,
      status: 'downloading',
      error: undefined,
      progress: 0,
      downloadedMb: 0,
      totalMb: getSummaryModelSizeMb(selectedSummaryModel || recommendedSummaryModel),
      speedMbps: 0,
    }));

    try {
      // Call download command directly (no retry command exists for built-in AI)
      const modelName = selectedSummaryModel;
      if (!modelName) {
        throw new Error('Summary model recommendation is not ready yet');
      }
      await invoke('builtin_ai_download_model', { modelName });
    } catch (error) {
      console.error('[DownloadProgressStep] Summary retry failed:', error);
      setSummaryState((prev) => ({
        ...prev,
        status: 'error',
        error: error instanceof Error ? error.message : 'Retry failed',
      }));

      toast.error(t('Summary model download retry failed'), {
        description: t('Please check your connection and try again.'),
      });
    } finally {
      // Allow retry again after 2 seconds
      setTimeout(() => {
        retryingSummaryRef.current = false;
      }, 2000);
    }
  };

  // Detect platform on mount
  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };

    checkPlatform();
  }, []);

  // Which variant a download would fetch, and how big it is. Read from the backend rather
  // than hardcoded so the card can't drift from `GigaamVariant::default()`.
  useEffect(() => {
    invoke<{
      selected: string;
      model_present: boolean;
      variants: { id: string; label: string; size_mb: number }[];
    }>('gigaam_status')
      .then((status) => {
        const selected = status.variants?.find((v) => v.id === status.selected);
        if (selected) {
          setTranscriptionModel({ label: selected.label, sizeMb: selected.size_mb });
          setTranscriptionState((prev) => ({
            ...prev,
            totalMb: prev.totalMb === TRANSCRIPTION_FALLBACK_MB ? selected.size_mb : prev.totalMb,
          }));
        }
        if (status.model_present) {
          setTranscriptionModelDownloaded(true);
          setTranscriptionState((prev) => ({ ...prev, status: 'completed', progress: 100 }));
        }
      })
      .catch((error) => console.warn('[DownloadProgressStep] gigaam_status failed:', error));
  }, [setTranscriptionModelDownloaded]);

  // Speaker-recognition models: real size and whether they are already on disk.
  useEffect(() => {
    invoke<{ available: boolean; download_mb: number; downloading: boolean }>('diarization_status')
      .then((status) => {
        setDiarizationState((prev) => ({
          ...prev,
          status: status.available ? 'completed' : status.downloading ? 'downloading' : prev.status,
          progress: status.available ? 100 : prev.progress,
          totalMb: status.download_mb || prev.totalMb,
        }));
      })
      .catch((error) => console.warn('[DownloadProgressStep] diarization_status failed:', error));
  }, []);

  const startDiarizationDownload = async () => {
    if (diarizationDownloadStartedRef.current) return;
    diarizationDownloadStartedRef.current = true;
    setDiarizationState((prev) => ({ ...prev, status: 'downloading', error: undefined }));
    try {
      await invoke('download_diarization_models');
    } catch (error) {
      console.error('Failed to download the speaker models:', error);
      diarizationDownloadStartedRef.current = false;
      setDiarizationState((prev) => ({ ...prev, status: 'error', error: String(error) }));
    }
  };

  const handleRetryDiarizationDownload = async () => {
    if (retryingDiarizationRef.current) return;
    retryingDiarizationRef.current = true;
    diarizationDownloadStartedRef.current = false;
    try {
      await startDiarizationDownload();
    } finally {
      setTimeout(() => {
        retryingDiarizationRef.current = false;
      }, 2000);
    }
  };

  // Speaker-model download progress. The backend reports bytes per file, and there are two
  // files, so the bar tracks the file in flight rather than the pair.
  useEffect(() => {
    const unlistenProgress = listen<{
      file: string;
      downloaded: number;
      total: number;
      percent: number;
    }>('diarization-download-progress', (event) => {
      const { downloaded, total, percent } = event.payload;
      setDiarizationState((prev) => ({
        ...prev,
        status: 'downloading',
        progress: percent,
        downloadedMb: downloaded / (1024 * 1024),
        totalMb: total > 0 ? total / (1024 * 1024) : prev.totalMb,
        speedMbps: 0,
      }));
    });

    const unlistenReady = listen('diarization-ready', () => {
      setDiarizationState((prev) => ({ ...prev, status: 'completed', progress: 100 }));
    });

    const unlistenError = listen<string>('diarization-download-error', (event) => {
      diarizationDownloadStartedRef.current = false;
      setDiarizationState((prev) => ({ ...prev, status: 'error', error: event.payload }));
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  // Downloads are OPT-IN. The transcription model can also be fetched later from
  // Settings → Transcription, and the local summary model is optional (cloud providers —
  // GigaChat/DeepSeek — can be used instead). Nothing auto-starts; the user triggers a
  // download or skips and continues.
  const startTranscriptionDownload = async () => {
    if (transcriptionDownloadStartedRef.current) return;
    transcriptionDownloadStartedRef.current = true;
    setTranscriptionState((prev) => ({ ...prev, status: 'downloading' }));
    try {
      await startBackgroundDownloads({ includeTranscription: true, includeSummary: false });
    } catch (error) {
      console.error('Failed to start the transcription model download:', error);
      transcriptionDownloadStartedRef.current = false;
      setTranscriptionState((prev) => ({ ...prev, status: 'error', error: String(error) }));
    }
  };

  // Listen to transcription model (GigaAM) download progress. `extracting` reports no byte
  // counts, so the bar holds its last value and the size line reads as a stage instead.
  useEffect(() => {
    const unlistenProgress = listen<{
      downloaded: number;
      total: number;
      percent: number;
      stage: string;
    }>('gigaam-download-progress', (event) => {
      const { downloaded, total, percent, stage } = event.payload;
      setTranscriptionState((prev) =>
        stage === 'downloading'
          ? {
              ...prev,
              status: 'downloading',
              progress: percent,
              downloadedMb: downloaded / (1024 * 1024),
              totalMb: total / (1024 * 1024),
              speedMbps: 0,
            }
          : { ...prev, status: 'downloading', progress: 100 },
      );
    });

    const unlistenComplete = listen('gigaam-ready', () => {
      setTranscriptionState((prev) => ({ ...prev, status: 'completed', progress: 100 }));
      setTranscriptionModelDownloaded(true);
    });

    const unlistenError = listen<string>('gigaam-download-error', (event) => {
      transcriptionDownloadStartedRef.current = false;
      setTranscriptionState((prev) => ({ ...prev, status: 'error', error: event.payload }));
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [setTranscriptionModelDownloaded]);

  // Listen to Summary Model download progress (always downloading for builtin-ai)
  useEffect(() => {
    const unlisten = listen<{
      model: string;
      progress: number;
      downloaded_mb?: number;
      total_mb?: number;
      speed_mbps?: number;
      status: string;
      error?: string;
    }>('builtin-ai-download-progress', (event) => {
      const { model, progress, downloaded_mb, total_mb, speed_mbps, status, error } = event.payload;
      if (selectedSummaryModel && model === selectedSummaryModel) {
        setSummaryState((prev) => ({
          ...prev,
          status: status === 'completed'
            ? 'completed'
            : status === 'error'
            ? 'error'
            : 'downloading',
          progress,
          downloadedMb: downloaded_mb ?? prev.downloadedMb,
          totalMb: (total_mb ?? prev.totalMb) || getSummaryModelSizeMb(model),
          speedMbps: speed_mbps ?? prev.speedMbps,
          error: status === 'error' ? error : undefined,
        }));

        if (status === 'completed' || progress >= 100) {
          setSummaryModelDownloaded(true);
        }
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [selectedSummaryModel]);

  useEffect(() => {
    const modelForSize = selectedSummaryModel || recommendedSummaryModel;
    if (!modelForSize) return;

    setSummaryState((prev) => ({
      ...prev,
      status: summaryModelDownloaded
        ? 'completed'
        : prev.status === 'completed'
        ? 'waiting'
        : prev.status,
      progress: summaryModelDownloaded
        ? 100
        : prev.status === 'completed'
        ? 0
        : prev.progress,
      totalMb: prev.totalMb || getSummaryModelSizeMb(modelForSize),
    }));
  }, [selectedSummaryModel, recommendedSummaryModel, summaryModelDownloaded]);

  const startSummaryDownload = async () => {
    if (summaryModelDownloaded) return;
    if (!selectedSummaryModel) {
      toast.info(t('Summary model not ready yet'), { description: t('Please try again in a moment.') });
      return;
    }
    if (summaryDownloadStartedRef.current) return;
    summaryDownloadStartedRef.current = true;
    try {
      setSummaryState((prev) => ({
        ...prev,
        status: 'downloading',
        totalMb: getSummaryModelSizeMb(selectedSummaryModel),
      }));
      await startBackgroundDownloads({
        includeTranscription: false,
        includeSummary: true,
        summaryModel: selectedSummaryModel,
      });
    } catch (error) {
      console.error('Failed to start summary model download:', error);
      summaryDownloadStartedRef.current = false;
      setSummaryState((prev) => ({ ...prev, status: 'error', error: String(error) }));
    }
  };

  const handleContinue = async () => {
    // Downloads are optional; sync transcription availability if a model happens to be
    // present, but never block continuing.
    try {
      const status = await invoke<{ model_present?: boolean }>('gigaam_status');
      if (status?.model_present && !transcriptionModelDownloaded) {
        setTranscriptionModelDownloaded(true);
        setTranscriptionState((prev) => ({ ...prev, status: 'completed', progress: 100 }));
      }
    } catch (error) {
      // No transcription model installed yet — expected; the user can fetch it in Settings.
      console.warn('[DownloadProgressStep] transcription model not available yet:', error);
    }

    // If a download is still running, let the user know it continues in the background.
    if (anyDownloading) {
      toast.info(t('Downloads will continue in the background'), {
        description: t('You can start using the app now.'),
        duration: 5000,
      });
    }

    if (isMac) {
      // macOS: Go to Permissions step (will complete after permissions granted)
      goNext();
    } else {
      // Non-macOS: Complete onboarding immediately (downloads continue in background)
      setIsCompleting(true);
      try {
        await completeOnboarding();

        // Small delay to ensure state is saved before reload
        await new Promise(resolve => setTimeout(resolve, 100));

        window.location.reload();
      } catch (error) {
        console.error('Failed to complete onboarding:', error);
        toast.error(t('Failed to complete setup'), {
          description: t('Please try again.'),
        });
        setIsCompleting(false);
      }
    }
  };

  const renderDownloadCard = (
    type: 'transcription' | 'summary' | 'diarization',
    title: string,
    icon: React.ReactNode,
    state: DownloadState,
    modelSize: string,
    sizeUnit = 'MB',
    onDownload?: () => void,
    hint?: string
  ) => (
    <div className="bg-background rounded-xl border border-border p-5">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center">
            {icon}
          </div>
          <div>
            <h3 className="font-medium text-foreground">{title}</h3>
            <p className="text-sm text-muted-foreground">{modelSize}</p>
            {hint && <p className="text-xs text-muted-foreground mt-0.5">{hint}</p>}
          </div>
        </div>
        <div>
          {state.status === 'waiting' && (
            onDownload ? (
              <button
                onClick={onDownload}
                className="flex items-center gap-1.5 h-8 px-3 rounded-md border border-border text-sm text-muted-foreground hover:bg-muted transition-colors"
              >
                <Download className="w-4 h-4" />
                {t('Download')}
              </button>
            ) : (
              <span className="text-xs text-muted-foreground">{t('Optional')}</span>
            )
          )}
          {state.status === 'downloading' && (
            <Loader2 className="w-5 h-5 text-muted-foreground animate-spin" />
          )}
          {state.status === 'completed' && (
            <div className="w-6 h-6 rounded-full bg-success/10 flex items-center justify-center">
              <Check className="w-4 h-4 text-success" />
            </div>
          )}
          {state.status === 'error' && (
            <span className="text-sm text-destructive">{t('Failed')}</span>
          )}
        </div>
      </div>

      {/* Progress Bar */}
      {(state.status === 'downloading' || state.status === 'completed') && (
        <div className="space-y-2">
          <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full rounded-full bg-muted-foreground transition-all duration-300"
              style={{ width: `${state.progress}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">
              {state.downloadedMb.toFixed(1)} {sizeUnit} / {state.totalMb.toFixed(1)} {sizeUnit}
            </span>
            <div className="flex items-center gap-2">
              {state.speedMbps > 0 && (
                <span className="text-muted-foreground">
                  {state.speedMbps.toFixed(1)} {sizeUnit}/s
                </span>
              )}
              <span className="font-semibold text-foreground">
                {Math.round(state.progress)}%
              </span>
            </div>
          </div>
        </div>
      )}

      {state.status === 'error' && state.error && (
        <div className="mt-2 p-3 bg-destructive/10 border border-destructive/40 rounded-md">
          <p className="text-sm text-destructive font-medium">{t('Download Error')}</p>
          <p className="text-xs text-destructive mt-1">{state.error}</p>
          {(type === 'transcription' || type === 'summary' || type === 'diarization') && (
            <button
              onClick={
                type === 'transcription'
                  ? handleRetryDownload
                  : type === 'summary'
                    ? handleRetrySummaryDownload
                    : handleRetryDiarizationDownload
              }
              className="mt-3 flex h-9 w-full items-center justify-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            >
              <RefreshCw size={16} />
              {t('Try Again')}
            </button>
          )}
        </div>
      )}
    </div>
  );

  return (
    <OnboardingContainer
      title={t('Getting things ready')}
      description={t('These local models are optional — download them now, or skip and configure transcription and cloud providers (GigaChat / DeepSeek) in Settings.')}
      step={3}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-6">
        {/* Download Cards */}
        <div className="w-full max-w-lg space-y-4">
          {renderDownloadCard(
            'transcription',
            t('Transcription Engine'),
            <Mic className="w-5 h-5 text-muted-foreground" />,
            transcriptionState,
            `${transcriptionModel.label} · ~${transcriptionModel.sizeMb} MB`,
            'MB',
            startTranscriptionDownload,
            t('Needed to transcribe on this device — you can also download it later in Settings → Transcription')
          )}

          {renderDownloadCard(
            'diarization',
            t('Speaker recognition'),
            <Users className="w-5 h-5 text-muted-foreground" />,
            diarizationState,
            `${t('Segmentation + embeddings')} · ~${Math.round(diarizationState.totalMb)} MB`,
            'MB',
            startDiarizationDownload,
            t('Labels who said what. Without it transcripts have no speaker names.')
          )}

          {renderDownloadCard(
            'summary',
            t('Summary Engine'),
            <Sparkles className="w-5 h-5 text-muted-foreground" />,
            summaryState,
            getSummaryModelSizeLabel(selectedSummaryModel || recommendedSummaryModel),
            'MiB',
            startSummaryDownload,
            t('Optional — or use GigaChat / DeepSeek (Settings → Providers)')
          )}
        </div>

        {/* Info Message */}
        <AnimatePresence>
          {(anyDownloading) && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              transition={{ duration: 0.3, ease: 'easeOut' }}
              className="w-full max-w-lg bg-muted rounded-lg p-4 text-sm text-foreground"
            >
              <div className="flex items-start gap-3">
                <Download className="w-5 h-5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <div>
                  <p className="font-medium">{t('You can continue while this finishes')}</p>
                  <p className="text-muted-foreground mt-1">{t('Downloads continue in the background.')}</p>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Continue Button — always available; downloads are optional */}
        <div className="w-full max-w-xs">
          <Button
            onClick={handleContinue}
            disabled={isCompleting}
            className="h-11 w-full rounded-full bg-primary text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isCompleting ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : anyDownloading ? (
              t('Continue in background')
            ) : transcriptionModelDownloaded || summaryModelDownloaded ? (
              t('Continue')
            ) : (
              t('Skip & continue')
            )}
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
