'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslations } from 'next-intl';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

export interface OrphanCheckpoint {
  meeting_folder: string;
  display_name: string;
  chunk_count: number;
  estimated_duration_seconds: number;
  last_modified_ms: number;
}

interface OrphanCheckpointDialogProps {
  orphans: OrphanCheckpoint[];
  onDismiss: () => void;
  onAction?: (meetingFolder: string) => void;
}

function formatDuration(seconds: number): string {
  const totalSeconds = Math.round(seconds);
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  if (m === 0) return `${s}s`;
  return `${m}m ${s.toString().padStart(2, '0')}s`;
}
export function OrphanCheckpointDialog({ orphans, onDismiss, onAction }: OrphanCheckpointDialogProps) {
  const t = useTranslations('recording');
  const [busy, setBusy] = useState<string | null>(null);

  const handleRecover = async (orphan: OrphanCheckpoint) => {
    setBusy(orphan.meeting_folder);
    try {
      // Reuse the existing command; 48000 = 48kHz sample rate (project default).
      const result = await invoke('recover_audio_from_checkpoints', {
        meetingFolder: orphan.meeting_folder,
        sampleRate: 48000,
      }) as { status: string; audio_file_path?: string; message: string };
      if (result.status === 'success' || result.status === 'partial') {
        toast.success(t('orphan_checkpoint_recovered'));
        onAction?.(orphan.meeting_folder);
      } else {
        toast.warning(t('orphan_checkpoint_recovered_failed') + ': ' + result.message);
      }
    } catch (e) {
      toast.error(t('orphan_checkpoint_recovered_failed') + ': ' + String(e));
    } finally {
      setBusy(null);
    }
  };

  const handleDiscard = async (orphan: OrphanCheckpoint) => {
    setBusy(orphan.meeting_folder);
    try {
      await invoke('discard_orphan_checkpoint_cmd', { meetingFolder: orphan.meeting_folder });
      toast.success(t('orphan_checkpoint_discarded'));
      onAction?.(orphan.meeting_folder);
    } catch (e) {
      toast.error(t('orphan_checkpoint_discarded_failed') + ': ' + String(e));
    } finally {
      setBusy(null);
    }
  };
  return (
    <Dialog open={orphans.length > 0} onOpenChange={(open) => { if (!open) onDismiss(); }}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('orphan_checkpoints_title')}</DialogTitle>
          <DialogDescription>{t('orphan_checkpoints_description')}</DialogDescription>
        </DialogHeader>

        <ul className="space-y-3 max-h-72 overflow-y-auto my-2">
          {orphans.map((o) => (
            <li key={o.meeting_folder} className="border border-gray-200 rounded-md p-3 bg-gray-50">
              <div className="font-medium text-gray-900 truncate">{o.display_name}</div>
              <div className="text-xs text-gray-500 mt-1 flex gap-3">
                <span>{t('orphan_checkpoint_chunks', { count: o.chunk_count })}</span>
                <span>{t('orphan_checkpoint_duration', { duration: formatDuration(o.estimated_duration_seconds) })}</span>
              </div>
              <div className="flex gap-2 mt-3">
                <Button
                  size="sm"
                  variant="default"
                  disabled={busy !== null}
                  onClick={() => handleRecover(o)}
                >
                  {busy === o.meeting_folder ? '...' : t('orphan_checkpoint_recover')}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy !== null}
                  onClick={() => handleDiscard(o)}
                >
                  {t('orphan_checkpoint_discard')}
                </Button>
              </div>
            </li>
          ))}
        </ul>

        <DialogFooter>
          <Button variant="ghost" onClick={onDismiss} disabled={busy !== null}>
            {t('orphan_checkpoints_dismiss')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
