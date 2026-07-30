"use client";

import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Copy, Loader2, MoreHorizontal, Pencil, Trash2 } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useT } from '@/lib/i18n';
import { useMeetingDrawer } from '@/contexts/MeetingDrawerContext';

/**
 * The "⋯" menu for the meeting conversation. Composed from shadcn DropdownMenu
 * primitives so focus management, keyboard navigation, positioning, and dismissal
 * follow the same accessible interaction model as the rest of the application.
 */

interface MeetingOverflowMenuProps {
  meetingId: string;
  hasSummary: boolean;
  onCopySummary: () => Promise<void> | void;
  onRenameMeeting: () => void;
}

export function MeetingOverflowMenu({
  meetingId,
  hasSummary,
  onCopySummary,
  onRenameMeeting,
}: MeetingOverflowMenuProps) {
  const t = useT();
  const meetingDrawer = useMeetingDrawer();
  const [open, setOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const closeMenu = () => setOpen(false);

  const handleDelete = useCallback(async () => {
    if (deleting) return;
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('Delete this meeting? This cannot be undone.'))) return;
    setDeleting(true);
    try {
      await invoke('api_delete_meeting', { meetingId, deleteRecordingFiles: false });
      toast.success(t('Meeting deleted'));
      closeMenu();
      meetingDrawer?.close();
    } catch (e) {
      console.error('Failed to delete meeting:', e);
      toast.error(`${t('Failed to delete meeting')}: ${String(e)}`);
    } finally {
      setDeleting(false);
    }
  }, [deleting, meetingDrawer, meetingId, t]);

  return (
    <div className="no-drag relative z-[1]">
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t('More actions')}
            title={t('More actions')}
            className="h-[38px] w-[38px] rounded-full bg-transparent shadow-none"
          >
            <MoreHorizontal size={18} />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          align="end"
          sideOffset={6}
          className="w-[248px] rounded-[14px] p-1.5"
        >
          <DropdownMenuItem
            onSelect={onRenameMeeting}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Pencil size={16} />
            <span>{t('Rename meeting')}</span>
          </DropdownMenuItem>

          <DropdownMenuSeparator className="mx-1.5 my-1" />

          <DropdownMenuItem
            disabled={!hasSummary}
            onSelect={() => void onCopySummary()}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Copy size={16} />
            <span>{t('Copy summary')}</span>
          </DropdownMenuItem>
          <DropdownMenuSeparator className="mx-1.5 my-1" />

          <DropdownMenuItem
            disabled={deleting}
            onSelect={() => void handleDelete()}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            {deleting ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
            <span>{t('Delete meeting')}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

    </div>
  );
}
