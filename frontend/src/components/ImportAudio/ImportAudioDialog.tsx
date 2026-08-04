import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Upload,
  Globe,
  Loader2,
  AlertCircle,
  CheckCircle2,
  Cpu,
  FileAudio,
  Clock,
  HardDrive,
  FolderOpen,
} from '@/components/deslop-icons';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/fluid-dialog';
import { Button } from '../ui/fluid-button';
import { Input } from '../ui/fluid-input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '../ui/fluid-select';
import { MaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useImportAudio, BatchImportResult, ExistingAudioMeeting, ImportResult } from '@/hooks/useImportAudio';
import { useRouter } from 'next/navigation';
import { useSidebar } from '../Sidebar/SidebarProvider';
import { getLanguageDisplayName, LANGUAGES } from '@/constants/languages';
import { useTranscriptionModels, ModelOption } from '@/hooks/useTranscriptionModels';
import { useT } from '@/lib/i18n';

type FluidIconProps = { size?: number; strokeWidth?: number; className?: string };
const UploadIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="upload" size={size} weight={400} className={className} />;
const CancelIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="close" size={size} weight={400} className={className} />;
const ExpandIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="expand_more" size={size} weight={400} className={className} />;
const CollapseIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="expand_less" size={size} weight={400} className={className} />;


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
        size="lg"
        className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={handleInteractOutside}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isProcessing ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-primary" />
                {t('Importing Audio...')}
              </>
            ) : error ? (
              <>
                <AlertCircle className="h-5 w-5 text-destructive" />
                {t('Import Failed')}
              </>
            ) : status === 'complete' ? (
              <>
                <CheckCircle2 className="h-5 w-5 text-success" />
                {t('Import Complete')}
              </>
            ) : (
              <>
                <Upload className="h-5 w-5 text-primary" />
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

        <div className="min-h-0 min-w-0 flex-1 space-y-4 overflow-x-hidden overflow-y-auto py-4">
          {/* File selection / info */}
          {!isProcessing && !error && (
            <>
              {batchFiles.length > 0 ? (
                <div className="bg-background rounded-lg p-4 space-y-3">
                  <div className="flex items-start gap-3">
                    <FolderOpen className="h-8 w-8 text-primary flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-foreground">
                        {batchFiles.length} {t('audio files selected')}
                      </p>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {importableBatchFiles.length} {t('new files')}
                        {duplicateBatchCount > 0 && ` · ${duplicateBatchCount} ${t('already imported or duplicated')}`}
                      </p>
                      <div className="flex items-center gap-4 text-sm text-muted-foreground mt-1">
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
                  <div className="max-h-28 overflow-y-auto text-xs text-muted-foreground space-y-1">
                    {batchFiles.slice(0, 20).map((file) => (
                      <div key={file.path} className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 flex-1 truncate">{file.filename}</span>
                        {(file.existing_meeting || file.duplicate_source_path) && (
                          <span className="flex-none text-primary">{t('duplicate')}</span>
                        )}
                      </div>
                    ))}
                    {batchFiles.length > 20 && (
                      <div>{t('and')} {batchFiles.length - 20} {t('more files')}</div>
                    )}
                  </div>
                  <Button variant="tertiary" size="md" onClick={handleSelectFolder} className="w-full">
                    {t('Choose Different Folder')}
                  </Button>
                </div>
              ) : fileInfo ? (
                <div className="bg-background rounded-lg p-4 space-y-3">
                  <div className="flex items-start gap-3">
                    <FileAudio className="h-8 w-8 text-primary flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-foreground truncate">{fileInfo.filename}</p>
                      <div className="flex items-center gap-4 text-sm text-muted-foreground mt-1">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3.5 w-3.5" />
                          {formatDuration(fileInfo.duration_seconds)}
                        </span>
                        <span className="flex items-center gap-1">
                          <HardDrive className="h-3.5 w-3.5" />
                          {formatFileSize(fileInfo.size_bytes)}
                        </span>
                        <span className="text-primary font-medium">{fileInfo.format}</span>
                      </div>
                    </div>
                  </div>

                  {fileInfo.existing_meeting && (
                    <div className="rounded-lg border border-primary/40 bg-primary/10 p-3">
                      <p className="text-sm font-medium text-foreground">{t('Audio already imported')}</p>
                      <p className="mt-1 text-xs text-muted-foreground">{fileInfo.existing_meeting.title}</p>
                      <Button
                        variant="tertiary"
                        size="md"
                        className="mt-3 w-full"
                        onClick={() => handleDuplicate(fileInfo.existing_meeting!)}
                      >
                        {t('Open existing meeting')}
                      </Button>
                    </div>
                  )}

                  {/* Editable title */}
                  <div className="space-y-1">
                    <label className="text-sm font-medium text-muted-foreground">{t('Meeting Title')}</label>
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

                  <Button variant="tertiary" size="md" onClick={handleSelectFile} className="w-full">
                    {t('Choose Different File')}
                  </Button>
                </div>
              ) : (
                <button
                  type="button"
                  className="flex min-h-48 w-full min-w-0 cursor-pointer items-center justify-center rounded-lg border-2 border-dashed border-border p-5 text-center transition-colors hover:bg-primary/[0.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-wait disabled:opacity-60 sm:p-8"
                  aria-label={t('Select Audio File')}
                  onClick={handleSelectFile}
                  disabled={status === 'validating'}
                >
                  <p className="text-sm text-muted-foreground">
                    {t('MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA')}
                  </p>
                </button>
              )}

              {/* Advanced options (collapsible) */}
              {(fileInfo || batchFiles.length > 0) && (
                <div className="border rounded-lg">
                  <Button
                    type="button"
                    variant="ghost"
                    size="md"
                    trailingIcon={showAdvanced ? CollapseIcon : ExpandIcon}
                    onClick={() => setShowAdvanced(!showAdvanced)}
                    className="w-full"
                  >
                    {t('Advanced Options')}
                  </Button>

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
                            <SelectTrigger className="w-full" placeholder={t('Select language')} />
                            <SelectContent className="max-h-60">
                              {LANGUAGES.map((lang, index) => (
                                <SelectItem key={lang.code} index={index} value={lang.code}>
                                  {getLanguageDisplayName(lang.code)}
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
                            <SelectTrigger className="w-full" placeholder={loadingModels ? t('Loading models...') : t('Select model')} />
                            <SelectContent>
                              {availableModels.map((model, index) => (
                                <SelectItem
                                  key={`${model.provider}:${model.name}`}
                                  index={index}
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
                <div className="rounded-lg border border-destructive/40 bg-destructive/10 p-3">
                  <p className="text-sm font-medium text-destructive">
                    {t('No transcription model is available for import')}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
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
                <div className="w-full bg-muted rounded-full h-3">
                  <div
                    className="bg-primary h-3 rounded-full transition-all duration-300 ease-out"
                    style={{ width: `${displayedProgress}%` }}
                  />
                </div>
                <div className="flex justify-between text-xs text-muted-foreground mt-1">
                  <span>{progress?.stage || batchProgress?.state}</span>
                  <span>{Math.round(displayedProgress)}%</span>
                </div>
              </div>
              {progress && <p className="text-sm text-muted-foreground text-center">{progress.message}</p>}
              {batchProgress && (
                <p className="text-sm text-muted-foreground text-center">
                  {t('File')} {batchProgress.current_index} {t('of')} {batchProgress.total}: {batchProgress.current_title}
                  {' · '}{batchProgress.completed} {t('imported')}, {batchProgress.skipped} {t('skipped')}, {batchProgress.truncated} {t('not attempted')}, {batchProgress.failed} {t('failed')}
                </p>
              )}
            </div>
          )}

          {/* Error display */}
          {error && (
            <div className="bg-destructive/10 border border-destructive/40 rounded-lg p-3">
              <p className="text-sm text-destructive">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter className="min-w-0 shrink-0 gap-2 sm:flex-wrap sm:space-x-0">
          {!isProcessing && !error && (
            <>
              <Button className="min-w-0" variant="ghost" size="md" onClick={() => onOpenChange(false)}>
                {t('Cancel')}
              </Button>
              <Button
                onClick={handleStartImport}
                className="min-w-0"
                variant="primary"
                size="md"
                leadingIcon={UploadIcon}
                disabled={
                  (!fileInfo && batchFiles.length === 0)
                  || Boolean(fileInfo?.existing_meeting)
                  || (batchFiles.length > 0 && importableBatchFiles.length === 0)
                  || loadingModels
                  || !selectedModel
                }
              >
                {batchFiles.length > 0 ? `${t('Import')} ${importableBatchFiles.length}` : t('Import')}
              </Button>
            </>
          )}
          {isProcessing && (
            <Button variant="ghost" size="md" leadingIcon={CancelIcon} onClick={handleCancel}>
              {t('Cancel')}
            </Button>
          )}
          {error && (
            <>
              <Button variant="ghost" size="md" onClick={() => onOpenChange(false)}>
                {t('Close')}
              </Button>
              <Button onClick={reset} variant="secondary" size="md">
                {t('Try Again')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
