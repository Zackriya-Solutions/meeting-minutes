'use client';

import { useCallback, useEffect, useState } from 'react';
import Link from 'next/link';
import { toast } from 'sonner';
import { listen } from '@tauri-apps/api/event';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { daysLeftLabel, type PurgeOutcome, type TrashItem } from '@/lib/organization';
import { organizationService } from '@/services/organizationService';

type PendingConfirm = { kind: 'one'; item: TrashItem } | { kind: 'all' } | null;

export default function TrashPage() {
  const { refetchMeetings, refetchOrganization } = useSidebar();
  const [items, setItems] = useState<TrashItem[]>([]);
  const [confirm, setConfirm] = useState<PendingConfirm>(null);

  const load = useCallback(async () => {
    try {
      setItems(await organizationService.listTrash());
    } catch (error) {
      toast.error('Could not load Trash', { description: String(error) });
    }
  }, []);

  useEffect(() => {
    void load();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen('meetings-changed', () => void load())
      .then(fn => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(error => console.error('Failed to listen for meetings-changed:', error));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  const refreshAll = () => Promise.all([load(), refetchMeetings(), refetchOrganization()]);

  const reportOutcome = (outcome: PurgeOutcome) => {
    if (outcome.failed.length) {
      toast.error(`${outcome.failed.length} meeting(s) could not be removed and stay in Trash`);
    } else {
      toast.success('Trash emptied');
    }
  };

  const restore = async (item: TrashItem) => {
    try {
      await organizationService.restoreMeeting(item.id);
      toast.success(`Restored "${item.title}"`);
    } catch (error) {
      toast.error(String(error));
    }
    await refreshAll();
  };

  const onConfirm = async () => {
    const pending = confirm;
    setConfirm(null);
    if (!pending) return;
    try {
      if (pending.kind === 'one') {
        await organizationService.deleteMeetingForever(pending.item.id);
        toast.success(`Deleted "${pending.item.title}"`);
      } else {
        reportOutcome(await organizationService.emptyTrash());
      }
    } catch (error) {
      toast.error(String(error));
    }
    await refreshAll();
  };

  return (
    <div className="flex h-screen flex-col bg-gray-50">
      <div className="flex items-center justify-between border-b border-gray-200 bg-white px-6 py-4">
        <h1 className="text-lg font-semibold text-gray-800">Trash</h1>
        <button
          type="button"
          disabled={items.length === 0}
          onClick={() => setConfirm({ kind: 'all' })}
          className="rounded-md border border-red-200 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 disabled:opacity-40"
        >
          Empty Trash
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-6">
        {items.length === 0 ? (
          <p className="text-sm text-gray-500">Trash is empty.</p>
        ) : (
          <ul className="divide-y divide-gray-200 rounded-lg border border-gray-200 bg-white">
            {items.map(item => (
              <li key={item.id} className="flex items-center gap-4 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <Link href={`/meeting-details?id=${item.id}`} className="block truncate text-sm font-medium text-gray-800 hover:underline">
                    {item.title}
                  </Link>
                  <p className="text-xs text-gray-500">
                    Trashed {new Date(item.deleted_at).toLocaleDateString()} · {daysLeftLabel(item.days_left)}
                  </p>
                </div>
                <button type="button" onClick={() => void restore(item)} className="rounded-md px-3 py-1 text-sm text-blue-600 hover:bg-blue-50">
                  Restore
                </button>
                <button type="button" onClick={() => setConfirm({ kind: 'one', item })} className="rounded-md px-3 py-1 text-sm text-red-600 hover:bg-red-50">
                  Delete forever
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <ConfirmationModal
        isOpen={confirm !== null}
        text={confirm?.kind === 'one'
          ? `Permanently delete "${confirm.item.title}" and its recording (if stored in the recordings folder)? This cannot be undone.`
          : `Permanently delete all ${items.length} meeting(s) in Trash and their recordings (if stored in the recordings folder)? This cannot be undone.`}
        onConfirm={() => void onConfirm()}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}
