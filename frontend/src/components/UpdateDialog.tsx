'use client';
import React, { useState, useEffect } from 'react';
import { Download, X, CheckCircle2, AlertCircle, Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { updateService, UpdateInfo, UpdateProgress } from '@/services/updateService';
import { check, Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { toast } from 'sonner';
import { useTranslations } from 'next-intl';

interface UpdateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  updateInfo: UpdateInfo | null;
}

export function UpdateDialog({ open, onOpenChange, updateInfo }: UpdateDialogProps) {
  const [isDownloading, setIsDownloading] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const t = useTranslations('settings');
  const [update, setUpdate] = useState<Update | null>(null);

  useEffect(() => {
    if (open && updateInfo?.available) {
      // Reset state when dialog opens
      setIsDownloading(false);
      setProgress(null);
      setError(null);

      // Get the update object when dialog opens
      check().then((updateResult) => {
        if (updateResult?.available) {
          setUpdate(updateResult);
        } else {
          setError(t('update.no_longer'));
        }
      }).catch((err) => {
        console.error('Failed to get update object:', err);
        setError(t('update.prepare_failed', { message: err.message || 'Unknown error' }));
      });
    } else {
      // Reset state when dialog closes
      setIsDownloading(false);
      setProgress(null);
      setError(null);
      setUpdate(null);
    }
  }, [open, updateInfo]);

  const handleDownloadAndInstall = async () => {
    // Get update object if not already available
    let updateToUse: Update | null = update;
    if (!updateToUse) {
      try {
        const updateResult = await check();
        if (updateResult?.available) {
          updateToUse = updateResult;
          setUpdate(updateResult);
        } else {
          setError(t('update.not_available'));
          return;
        }
      } catch (err: any) {
        setError(t('update.get_failed', { message: err.message || 'Unknown error' }));
        return;
      }
    }

    // At this point, updateToUse is guaranteed to be non-null
    if (!updateToUse) {
      return; // This should never happen, but TypeScript needs this check
    }

    setIsDownloading(true);
    setError(null);
    setProgress({ downloaded: 0, total: 0, percentage: 0 });

    try {
      let downloaded = 0;
      let contentLength = 0;

      // Use the official Tauri updater API with progress callbacks
      await updateToUse.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength || 0;
            console.log(`[UpdateDialog] Started downloading ${contentLength} bytes`);
            setProgress({
              downloaded: 0,
              total: contentLength,
              percentage: 0,
            });
            break;

          case 'Progress':
            downloaded += event.data.chunkLength || 0;
            const percentage = contentLength > 0
              ? Math.round((downloaded / contentLength) * 100)
              : 0;
            console.log(`[UpdateDialog] Progress: ${downloaded} / ${contentLength} bytes (${percentage}%)`);
            setProgress({
              downloaded,
              total: contentLength,
              percentage,
            });
            break;

          case 'Finished':
            console.log('[UpdateDialog] Download finished');
            setProgress({
              downloaded: contentLength,
              total: contentLength,
              percentage: 100,
            });
            break;
        }
      });

      console.log('[UpdateDialog] Update installed successfully');
      toast.success(t('update.installed_toast'));

      // Mark download as complete before closing
      setIsDownloading(false);

      // Close dialog before relaunch
      handleOpenChange(false);

      // Relaunch the app
      await relaunch();
    } catch (err: any) {
      console.error('Update failed:', err);
      setError(err.message || 'Failed to download or install update');
      setIsDownloading(false);
      toast.error(t('update.failed_toast', { message: err.message || 'Unknown error' }));
    }
  };

  const formatDate = (dateString?: string) => {
    if (!dateString) return '';
    try {
      return new Date(dateString).toLocaleDateString();
    } catch {
      return dateString;
    }
  };

  // Prevent closing the dialog when downloading
  const handleOpenChange = (newOpen: boolean) => {
    // If trying to close while downloading, prevent it
    if (!newOpen && isDownloading) {
      return;
    }
    // Otherwise, allow normal close behavior
    onOpenChange(newOpen);
  };

  // Prevent ESC key from closing dialog during download
  const handleEscapeKeyDown = (event: KeyboardEvent) => {
    if (isDownloading) {
      event.preventDefault();
    }
  };

  // Prevent outside clicks from closing dialog during download
  const handleInteractOutside = (event: Event) => {
    if (isDownloading) {
      event.preventDefault();
    }
  };

  if (!updateInfo?.available) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="sm:max-w-[500px]"
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={handleInteractOutside}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isDownloading ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-blue-600" />
                {t('update.downloading_title')}
              </>
            ) : error ? (
              <>
                <AlertCircle className="h-5 w-5 text-red-600" />
                {t('update.error_title')}
              </>
            ) : (
              <>
                <Download className="h-5 w-5 text-blue-600" />
                {t('update.available_title')}
              </>
            )}
          </DialogTitle>
          <DialogDescription>
            {isDownloading
              ? t('update.downloading_desc')
              : error
              ? t('update.error_desc')
              : t('update.available_desc', { version: updateInfo.version ?? '' })}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {!isDownloading && !error && (
            <>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">{t('update.current_version')}</span>
                  <span className="font-medium">{updateInfo.currentVersion}</span>
                </div>
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">{t('update.new_version')}</span>
                  <span className="font-medium text-blue-600">{updateInfo.version}</span>
                </div>
                {updateInfo.date && (
                  <div className="flex justify-between text-sm">
                    <span className="text-muted-foreground">{t('update.release_date')}</span>
                    <span className="font-medium">{formatDate(updateInfo.date)}</span>
                  </div>
                )}
              </div>

              {updateInfo.body && (
                <div className="bg-gray-50 rounded-lg p-3 max-h-40 overflow-y-auto">
                  <p className="text-sm text-gray-700 whitespace-pre-wrap">
                    {updateInfo.body}
                  </p>
                </div>
              )}
            </>
          )}

          {isDownloading && progress && (
            <div className="space-y-2">
              <div className="relative">
                <div className="w-full bg-gray-200 rounded-full h-3">
                  <div
                    className="bg-blue-600 h-3 rounded-full transition-all duration-300 ease-out"
                    style={{ width: `${Math.min(progress.percentage, 100)}%` }}
                  />
                </div>
                <div className="flex justify-between text-xs text-gray-600 mt-1">
                  <span>{t('update.percent_complete', { percent: Math.round(progress.percentage) })}</span>
                  {progress.total > 0 && (
                    <span>
                      {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                    </span>
                  )}
                </div>
              </div>
              <p className="text-sm text-muted-foreground text-center">
                {t('update.will_restart')}
              </p>
            </div>
          )}

          {error && (
            <div className="bg-red-50 border border-red-200 rounded-lg p-3">
              <p className="text-sm text-red-800">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter>
          {!isDownloading && !error && (
            <>
              <Button variant="outline" onClick={() => handleOpenChange(false)}>
                {t('update.later')}
              </Button>
              <Button onClick={handleDownloadAndInstall} className="bg-blue-600 hover:bg-blue-700">
                <Download className="h-4 w-4 mr-2" />
                {t('update.download_install')}
              </Button>
            </>
          )}
          {error && (
            <Button variant="outline" onClick={() => handleOpenChange(false)}>
              {t('update.close')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
}
