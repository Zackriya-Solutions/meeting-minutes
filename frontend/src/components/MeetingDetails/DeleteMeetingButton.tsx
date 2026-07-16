"use client";

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { Trash2 } from '@/components/memento/LucideCompat';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { clearMarkedMoments } from '@/lib/markedMoments';
import { useT } from '@/lib/i18n';

interface DeleteMeetingResponse {
  status: 'success';
  message: string;
  files_deleted: boolean;
  files_warning?: string | null;
}

interface DeleteMeetingButtonProps {
  meetingId?: string;
  meetingFolderPath?: string | null;
}

export function DeleteMeetingButton({
  meetingId,
  meetingFolderPath,
}: DeleteMeetingButtonProps) {
  const t = useT();
  const router = useRouter();
  const {
    currentMeeting,
    setCurrentMeeting,
    refetchMeetings,
    stopSummaryPolling,
  } = useSidebar();
  const [open, setOpen] = useState(false);
  const [deleteRecordingFiles, setDeleteRecordingFiles] = useState(Boolean(meetingFolderPath));
  const [isDeleting, setIsDeleting] = useState(false);

  if (!meetingId) return null;

  const handleOpenChange = (nextOpen: boolean) => {
    if (isDeleting) return;
    setOpen(nextOpen);
    if (nextOpen) {
      setDeleteRecordingFiles(Boolean(meetingFolderPath));
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      const result = await invoke<DeleteMeetingResponse>('api_delete_meeting', {
        meetingId,
        deleteRecordingFiles: deleteRecordingFiles && Boolean(meetingFolderPath),
      });

      stopSummaryPolling(meetingId);
      clearMarkedMoments(meetingId);
      await refetchMeetings();
      Analytics.trackMeetingDeleted(meetingId);

      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: 'intro-call', title: t('+ New Call') });
      }

      if (result.files_warning) {
        toast.warning(t('Meeting deleted, but recording files were kept'), {
          description: t('You can remove the recording folder manually in Finder.'),
        });
      } else if (result.files_deleted) {
        toast.success(t('Meeting and recording files deleted'));
      } else {
        toast.success(t('Meeting deleted from Memento'), {
          description: t('The recording folder and audio files were kept on this Mac.'),
        });
      }

      setOpen(false);
      router.push('/');
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      toast.error(t('Failed to delete meeting'), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="xl:px-4 hover:border-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] hover:text-[var(--danger)]"
        onClick={() => setOpen(true)}
        title={t('Delete meeting')}
      >
        <Trash2 className="xl:mr-2" size={18} />
        <span>{t('Delete')}</span>
      </Button>

      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t('Delete this meeting?')}</DialogTitle>
            <DialogDescription>
              {t('The transcript, summary, speakers, and other meeting data will be removed from Memento. This action cannot be undone.')}
            </DialogDescription>
          </DialogHeader>

          {meetingFolderPath ? (
            <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-[var(--border-strong)] bg-[var(--bg-sheet)] p-4">
              <input
                type="checkbox"
                checked={deleteRecordingFiles}
                onChange={(event) => setDeleteRecordingFiles(event.target.checked)}
                className="mt-1 h-4 w-4 accent-[var(--danger)]"
              />
              <span className="space-y-1">
                <span className="block text-sm font-semibold text-[var(--fg1)]">
                  {t('Also delete the recording folder and audio from this Mac')}
                </span>
                <span className="block break-all text-xs leading-relaxed text-[var(--fg2)]">
                  {meetingFolderPath}
                </span>
              </span>
            </label>
          ) : (
            <div className="rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4 text-sm text-[var(--fg2)]">
              {t('No recording folder is attached to this meeting. Only its data in Memento will be deleted.')}
            </div>
          )}

          <DialogFooter className="gap-2 sm:space-x-0">
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
              disabled={isDeleting}
            >
              {t('Cancel')}
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={handleDelete}
              disabled={isDeleting}
            >
              {isDeleting ? t('Deleting...') : t('Delete meeting')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
