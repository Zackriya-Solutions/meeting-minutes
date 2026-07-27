import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Upload,
  Globe,
  Loader2,
  AlertCircle,
  CheckCircle2,
  X,
  Cpu,
  FileAudio,
  Clock,
  HardDrive,
  FolderOpen,
  ChevronDown,
  ChevronUp,
} from '@/components/memento/LucideCompat';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useImportAudio, BatchImportResult, ExistingAudioMeeting, ImportResult } from '@/hooks/useImportAudio';
import { useRouter } from 'next/navigation';
import { useSidebar } from '../Sidebar/SidebarProvider';
import { LANGUAGES } from '@/constants/languages';
import { useTranscriptionModels, ModelOption } from '@/hooks/useTranscriptionModels';
import { useT } from '@/lib/i18n';


interface ImportAudioDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  preselectedFile?: string | null;
  onComplete?: () => void;
}

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function ImportAudioDialog({
  open,
  onOpenChange,
  preselectedFile,
  onComplete,
}: ImportAudioDialogProps) {
  const t = useT();
  const router = useRouter();
  const { refetchMeetings } = useSidebar();
  const { selectedLanguage, transcriptModelConfig } = useConfig();

  const [title, setTitle] = useState('');
  const [selectedLang, setSelectedLang] = useState(selectedLanguage || 'auto');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);

  // Always start as false — represents "dialog has not yet been opened".
  // Do NOT initialize from the `open` prop: if the component mounts with open=true
  // (e.g. drag-drop path), we still need the initialization effect to run.
  const prevOpenRef = useRef(false);

  // Use centralized model fetching hook
  const {
    availableModels,
    selectedModelKey,
    setSelectedModelKey,
    loadingModels,
    fetchModels,
    resetSelection,
  } = useTranscriptionModels(transcriptModelConfig);

  const handleImportComplete = useCallback((result: ImportResult) => {
    toast.success(`${t('Import complete!')} ${result.segments_count} ${t('segments created.')}`);

    // Refresh meetings list then navigate to the imported meeting
    refetchMeetings();
    onComplete?.();
    onOpenChange(false);
    router.push(`/meeting-details?id=${result.meeting_id}`);
  }, [t, router, refetchMeetings, onComplete, onOpenChange]);

  const handleImportError = useCallback((error: string) => {
    toast.error(t('Import failed'), { description: error });
  }, [t]);

  const handleDuplicate = useCallback((existing: ExistingAudioMeeting) => {
    toast.info(t('Audio already imported'), {
      description: `${t('Opening existing meeting')}: ${existing.title}`,
    });
    refetchMeetings();
    onOpenChange(false);
    router.push(`/meeting-details?id=${existing.meeting_id}`);
  }, [t, router, refetchMeetings, onOpenChange]);

  const handleBatchImportComplete = useCallback((result: BatchImportResult) => {
    const description = `${result.imported.length} ${t('imported')}, ${result.skipped.length} ${t('skipped')}, ${result.truncated.length} ${t('not attempted')}, ${result.failed.length} ${t('failed')}`;
    if (result.cancelled) {
      toast.info(t('Batch import cancelled'), { description });
    } else if (result.failed.length > 0) {
      toast.warning(t('Batch import completed with errors'), { description });
    } else {
      toast.success(t('Batch import complete'), { description });
    }
    refetchMeetings();
    onComplete?.();
    onOpenChange(false);
  }, [t, refetchMeetings, onComplete, onOpenChange]);

  const {
    status,
    fileInfo,
    batchFiles,
    progress,
    batchProgress,
    error,
    isProcessing,
    isBusy,
    selectFile,
    selectFolder,
    validateFile,
    startImport,
    startBatchImport,
    cancelImport,
    reset,
  } = useImportAudio({
    onComplete: handleImportComplete,
    onDuplicate: handleDuplicate,
    onBatchComplete: handleBatchImportComplete,
    onError: handleImportError,
  });

  // Reset state only when dialog transitions from closed to open
  // This prevents re-initialization when config changes while dialog is already open (Bug #4 & #5)
  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    // Only initialize when transitioning from closed (false) to open (true)
    if (open && !wasOpen) {
      reset();
      resetSelection();
      setTitle('');
      setTitleModifiedByUser(false);
      setSelectedLang(selectedLanguage || 'auto');
      setShowAdvanced(false);

      // Validate preselected file if provided
      if (preselectedFile) {
        validateFile(preselectedFile).then((info) => {
          if (info) {
            setTitle(info.filename);
          }
        });
      }

      // Fetch available models using centralized hook
      fetchModels();
    }
  }, [open, preselectedFile, selectedLanguage, transcriptModelConfig, reset, resetSelection, validateFile, fetchModels]);

  // Update title when fileInfo changes
  useEffect(() => {
    if (fileInfo && !title && !titleModifiedByUser) {
      setTitle(fileInfo.filename);
    }
  }, [fileInfo, title, titleModifiedByUser]);

  const selectedModel = useMemo((): ModelOption | undefined => {
    if (!selectedModelKey) return undefined;
    const colonIndex = selectedModelKey.indexOf(':');
    if (colonIndex === -1) return undefined;
    const provider = selectedModelKey.slice(0, colonIndex);
    const name = selectedModelKey.slice(colonIndex + 1);
    return availableModels.find((m) => m.provider === provider && m.name === name);
  }, [selectedModelKey, availableModels]);
  const isParakeetModel = selectedModel?.provider === 'parakeet';
  const isGigaamModel = selectedModel?.provider === 'gigaam';
  const isSaluteSpeechModel = selectedModel?.provider === 'salutespeech';
  // GigaAM (Russian e2e), Parakeet, and SaluteSpeech don't take a language hint here —
  // they always auto-detect (SaluteSpeech transcribes Russian in the cloud).
  const languageAutoOnly = isParakeetModel || isGigaamModel || isSaluteSpeechModel;
  const batchProcessed = batchProgress
    ? batchProgress.completed + batchProgress.skipped + batchProgress.truncated + batchProgress.failed
    : 0;
  const displayedProgress = Math.min(
    progress?.progress_percentage
      ?? (batchProgress && batchProgress.total > 0
        ? (batchProcessed / batchProgress.total) * 100
        : 0),
    100,
  );
  const importableBatchFiles = useMemo(
    () => batchFiles.filter((file) => !file.existing_meeting && !file.duplicate_source_path),
    [batchFiles],
  );
  const duplicateBatchCount = batchFiles.length - importableBatchFiles.length;

  useEffect(() => {
    if (languageAutoOnly && selectedLang !== 'auto') {
      setSelectedLang('auto');
    }
  }, [languageAutoOnly, selectedLang]);

  const handleSelectFile = async () => {
    const info = await selectFile();
    if (info) {
      setTitle(info.filename);
    }
  };

  const handleSelectFolder = async () => {
    await selectFolder();
  };

  const handleStartImport = async () => {
    if (loadingModels || !selectedModel) {
      toast.error(t('No transcription model is available for import'), {
        description: t('Download GigaAM in Settings → Transcription before importing audio.'),
      });
      return;
    }
    if (batchFiles.length > 0) {
      await startBatchImport(
        importableBatchFiles.map((file) => ({
          source_path: file.path,
          title: file.filename.replace(/__[0-9a-f]{8}$/i, ''),
        })),
        languageAutoOnly ? null : selectedLang === 'auto' ? null : selectedLang,
        selectedModel?.name || null,
        selectedModel?.provider || null
      );
      return;
    }
    if (!fileInfo) return;

    await startImport(
      fileInfo.path,
      title || fileInfo.filename,
      languageAutoOnly ? null : selectedLang === 'auto' ? null : selectedLang,
      selectedModel?.name || null,
      selectedModel?.provider || null
    );
  };

  const handleCancel = async () => {
    if (isProcessing) {
      await cancelImport();
      toast.info(t('Import cancelled'));
    }
    onOpenChange(false);
  };

  // Prevent closing during processing
  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && isProcessing) {
      return;
    }
    onOpenChange(newOpen);
  };

  const handleEscapeKeyDown = (event: KeyboardEvent) => {
    if (isProcessing) {
      event.preventDefault();
    }
  };

  const handleInteractOutside = (event: Event) => {
    if (isProcessing) {
      event.preventDefault();
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="max-h-[calc(100vh-2rem)] w-[calc(100vw-2rem)] max-w-[760px] overflow-x-hidden overflow-y-auto sm:max-w-[760px]"
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={handleInteractOutside}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isProcessing ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-[var(--gold)]" />
                {t('Importing Audio...')}
              </>
            ) : error ? (
              <>
                <AlertCircle className="h-5 w-5 text-[var(--danger)]" />
                {t('Import Failed')}
              </>
            ) : status === 'complete' ? (
              <>
                <CheckCircle2 className="h-5 w-5 text-[var(--success)]" />
                {t('Import Complete')}
              </>
            ) : (
              <>
                <Upload className="h-5 w-5 text-[var(--gold)]" />
                {t('Import Audio File')}
              </>
            )}
          </DialogTitle>
          <DialogDescription>
            {isProcessing
              ? progress?.message || t('Processing audio...')
              : error
              ? t('An error occurred during import')
              : t('Import an audio file to create a new meeting with transcripts')}
          </DialogDescription>
        </DialogHeader>

        <div className="min-w-0 space-y-4 py-4">
          {/* File selection / info */}
          {!isProcessing && !error && (
            <>
              {batchFiles.length > 0 ? (
                <div className="bg-[var(--bg-sheet)] rounded-lg p-4 space-y-3">
                  <div className="flex items-start gap-3">
                    <FolderOpen className="h-8 w-8 text-[var(--gold)] flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-[var(--fg1)]">
                        {batchFiles.length} {t('audio files selected')}
                      </p>
                      <p className="mt-1 text-xs text-[var(--fg2)]">
                        {importableBatchFiles.length} {t('new files')}
                        {duplicateBatchCount > 0 && ` · ${duplicateBatchCount} ${t('already imported or duplicated')}`}
                      </p>
                      <div className="flex items-center gap-4 text-sm text-[var(--fg2)] mt-1">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3.5 w-3.5" />
                          {formatDuration(batchFiles.reduce((sum, file) => sum + file.duration_seconds, 0))}
                        </span>
                        <span className="flex items-center gap-1">
                          <HardDrive className="h-3.5 w-3.5" />
                          {formatFileSize(batchFiles.reduce((sum, file) => sum + file.size_bytes, 0))}
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className="max-h-28 overflow-y-auto text-xs text-[var(--fg2)] space-y-1">
                    {batchFiles.slice(0, 20).map((file) => (
                      <div key={file.path} className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 flex-1 truncate">{file.filename}</span>
                        {(file.existing_meeting || file.duplicate_source_path) && (
                          <span className="flex-none text-[var(--gold)]">{t('duplicate')}</span>
                        )}
                      </div>
                    ))}
                    {batchFiles.length > 20 && (
                      <div>{t('and')} {batchFiles.length - 20} {t('more files')}</div>
                    )}
                  </div>
                  <Button variant="outline" size="sm" onClick={handleSelectFolder} className="w-full">
                    {t('Choose Different Folder')}
                  </Button>
                </div>
              ) : fileInfo ? (
                <div className="bg-[var(--bg-sheet)] rounded-lg p-4 space-y-3">
                  <div className="flex items-start gap-3">
                    <FileAudio className="h-8 w-8 text-[var(--gold)] flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-[var(--fg1)] truncate">{fileInfo.filename}</p>
                      <div className="flex items-center gap-4 text-sm text-[var(--fg2)] mt-1">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3.5 w-3.5" />
                          {formatDuration(fileInfo.duration_seconds)}
                        </span>
                        <span className="flex items-center gap-1">
                          <HardDrive className="h-3.5 w-3.5" />
                          {formatFileSize(fileInfo.size_bytes)}
                        </span>
                        <span className="text-[var(--gold)] font-medium">{fileInfo.format}</span>
                      </div>
                    </div>
                  </div>

                  {fileInfo.existing_meeting && (
                    <div className="rounded-lg border border-[color-mix(in_srgb,var(--gold)_45%,transparent)] bg-[color-mix(in_srgb,var(--gold)_10%,transparent)] p-3">
                      <p className="text-sm font-medium text-[var(--fg1)]">{t('Audio already imported')}</p>
                      <p className="mt-1 text-xs text-[var(--fg2)]">{fileInfo.existing_meeting.title}</p>
                      <Button
                        variant="outline"
                        size="sm"
                        className="mt-3 w-full"
                        onClick={() => handleDuplicate(fileInfo.existing_meeting!)}
                      >
                        {t('Open existing meeting')}
                      </Button>
                    </div>
                  )}

                  {/* Editable title */}
                  <div className="space-y-1">
                    <label className="text-sm font-medium text-[var(--fg2)]">{t('Meeting Title')}</label>
                    <Input
                      value={title}
                      onChange={(e) => {
                        setTitle(e.target.value);
                        setTitleModifiedByUser(true);
                      }}
                      placeholder={t('Enter meeting title')}
                      disabled={Boolean(fileInfo.existing_meeting)}
                    />
                  </div>

                  <Button variant="outline" size="sm" onClick={handleSelectFile} className="w-full">
                    {t('Choose Different File')}
                  </Button>
                </div>
              ) : (
                <div className="min-w-0 border-2 border-dashed border-[var(--border-strong)] rounded-lg p-5 text-center sm:p-8">
                  <FileAudio className="h-12 w-12 text-[var(--fg3)] mx-auto mb-4" />
                  <div className="flex min-w-0 flex-col justify-center gap-2 sm:flex-row sm:flex-wrap">
                    <Button className="min-w-0 whitespace-normal" onClick={handleSelectFile} disabled={status === 'validating'}>
                      {status === 'validating' ? (
                        <>
                          <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                          {t('Validating...')}
                        </>
                      ) : (
                        <>
                          <Upload className="h-4 w-4 mr-2" />
                          {t('Select Audio File')}
                        </>
                      )}
                    </Button>
                    <Button className="min-w-0 whitespace-normal" variant="outline" onClick={handleSelectFolder} disabled={status === 'validating'}>
                      <FolderOpen className="h-4 w-4 mr-2" />
                      {t('Select Audio Folder')}
                    </Button>
                  </div>
                  <p className="text-sm text-[var(--fg2)] mt-2">{t('MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA')}</p>
                </div>
              )}

              {/* Advanced options (collapsible) */}
              {(fileInfo || batchFiles.length > 0) && (
                <div className="border rounded-lg">
                  <button
                    onClick={() => setShowAdvanced(!showAdvanced)}
                    className="w-full flex items-center justify-between p-3 text-sm font-medium text-[var(--fg2)] hover:bg-[var(--bg-sheet)]"
                  >
                    <span>{t('Advanced Options')}</span>
                    {showAdvanced ? (
                      <ChevronUp className="h-4 w-4" />
                    ) : (
                      <ChevronDown className="h-4 w-4" />
                    )}
                  </button>

                  {showAdvanced && (
                    <div className="p-3 pt-0 space-y-4 border-t">
                      {/* Language selector */}
                      {!languageAutoOnly ? (
                        <div className="space-y-2">
                          <div className="flex items-center gap-2">
                            <Globe className="h-4 w-4 text-muted-foreground" />
                            <span className="text-sm font-medium">{t('Language')}</span>
                          </div>
                          <Select value={selectedLang} onValueChange={setSelectedLang}>
                            <SelectTrigger className="w-full">
                              <SelectValue placeholder={t('Select language')} />
                            </SelectTrigger>
                            <SelectContent className="max-h-60">
                              {LANGUAGES.map((lang) => (
                                <SelectItem key={lang.code} value={lang.code}>
                                  {lang.name}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>
                      ) : (
                        <div className="space-y-2">
                          <div className="flex items-center gap-2">
                            <Globe className="h-4 w-4 text-muted-foreground" />
                            <span className="text-sm font-medium">{t('Language')}</span>
                          </div>
                          <p className="text-xs text-muted-foreground">
                            {isSaluteSpeechModel
                              ? t('SaluteSpeech transcribes Russian in the Sber cloud and always auto-detects — no language selection needed.')
                              : isGigaamModel
                                ? t('GigaAM transcribes Russian and always auto-detects — no language selection needed.')
                                : t("Language selection isn't supported for Parakeet. It always uses automatic detection.")}
                          </p>
                        </div>
                      )}

                      {/* Model selector */}
                      {availableModels.length > 0 && (
                        <div className="space-y-2">
                          <div className="flex items-center gap-2">
                            <Cpu className="h-4 w-4 text-muted-foreground" />
                            <span className="text-sm font-medium">{t('Model')}</span>
                          </div>
                          <Select
                            value={selectedModelKey}
                            onValueChange={setSelectedModelKey}
                            disabled={loadingModels}
                          >
                            <SelectTrigger className="w-full">
                              <SelectValue placeholder={loadingModels ? t('Loading models...') : t('Select model')} />
                            </SelectTrigger>
                            <SelectContent>
                              {availableModels.map((model) => (
                                <SelectItem
                                  key={`${model.provider}:${model.name}`}
                                  value={`${model.provider}:${model.name}`}
                                >
                                  {model.displayName}
                                  {model.size_mb > 0 ? ` (${Math.round(model.size_mb)} ${t('MB')})` : ''}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}

              {(fileInfo || batchFiles.length > 0) && !loadingModels && !selectedModel && (
                <div className="rounded-lg border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] p-3">
                  <p className="text-sm font-medium text-[var(--danger)]">
                    {t('No transcription model is available for import')}
                  </p>
                  <p className="mt-1 text-xs text-[var(--fg2)]">
                    {t('Download GigaAM in Settings → Transcription before importing audio.')}
                  </p>
                </div>
              )}
            </>
          )}

          {/* Progress display */}
          {isProcessing && (progress || batchProgress) && (
            <div className="space-y-2">
              <div className="relative">
                <div className="w-full bg-[var(--bg-elevated)] rounded-full h-3">
                  <div
                    className="bg-[var(--gold)] h-3 rounded-full transition-all duration-300 ease-out"
                    style={{ width: `${displayedProgress}%` }}
                  />
                </div>
                <div className="flex justify-between text-xs text-[var(--fg2)] mt-1">
                  <span>{progress?.stage || batchProgress?.state}</span>
                  <span>{Math.round(displayedProgress)}%</span>
                </div>
              </div>
              {progress && <p className="text-sm text-muted-foreground text-center">{progress.message}</p>}
              {batchProgress && (
                <p className="text-sm text-[var(--fg2)] text-center">
                  {t('File')} {batchProgress.current_index} {t('of')} {batchProgress.total}: {batchProgress.current_title}
                  {' · '}{batchProgress.completed} {t('imported')}, {batchProgress.skipped} {t('skipped')}, {batchProgress.truncated} {t('not attempted')}, {batchProgress.failed} {t('failed')}
                </p>
              )}
            </div>
          )}

          {/* Error display */}
          {error && (
            <div className="bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] rounded-lg p-3">
              <p className="text-sm text-[var(--danger)]">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter className="min-w-0 gap-2 sm:flex-wrap sm:space-x-0">
          {!isProcessing && !error && (
            <>
              <Button className="min-w-0" variant="outline" onClick={() => onOpenChange(false)}>
                {t('Cancel')}
              </Button>
              <Button
                onClick={handleStartImport}
                className="min-w-0 whitespace-normal bg-[var(--gold)] hover:bg-[var(--gold-active)]"
                disabled={
                  (!fileInfo && batchFiles.length === 0)
                  || Boolean(fileInfo?.existing_meeting)
                  || (batchFiles.length > 0 && importableBatchFiles.length === 0)
                  || loadingModels
                  || !selectedModel
                }
              >
                <Upload className="h-4 w-4 mr-2" />
                {batchFiles.length > 0 ? `${t('Import')} ${importableBatchFiles.length}` : t('Import')}
              </Button>
            </>
          )}
          {isProcessing && (
            <Button variant="outline" onClick={handleCancel}>
              <X className="h-4 w-4 mr-2" />
              {t('Cancel')}
            </Button>
          )}
          {error && (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                {t('Close')}
              </Button>
              <Button onClick={reset} variant="outline">
                {t('Try Again')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
