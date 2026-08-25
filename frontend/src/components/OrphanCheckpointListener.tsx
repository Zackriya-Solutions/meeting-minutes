'use client';

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { OrphanCheckpointDialog, type OrphanCheckpoint } from './OrphanCheckpointDialog';

// PR-33: Listens for the `orphan-checkpoints-detected` event emitted by the Rust
// startup hook and renders the recovery dialog. Each orphan is removed from the
// list once the user recovers or discards it.

export function OrphanCheckpointListener() {
  const [orphans, setOrphans] = useState<OrphanCheckpoint[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    (async () => {
      const fn = await listen<OrphanCheckpoint[]>('orphan-checkpoints-detected', (event) => {
        if (cancelled) return;
        if (Array.isArray(event.payload) && event.payload.length > 0) {
          setOrphans((prev) => {
            const seen = new Set(prev.map((o) => o.meeting_folder));
            const merged = [...prev];
            for (const o of event.payload) {
              if (!seen.has(o.meeting_folder)) merged.push(o);
            }
            return merged;
          });
        }
      });
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    })().catch((e) => {
      // Non-fatal: dialog simply will not appear this session.
      console.warn('orphan-checkpoints listener failed', e);
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  const handleActioned = (meetingFolder: string) => {
    setOrphans((prev) => prev.filter((o) => o.meeting_folder !== meetingFolder));
  };

  const handleDismissAll = () => {
    setOrphans([]);
  };

  // The Dialog renders for the FIRST orphan only (one at a time); once the user
  // resolves it, the next orphan in the list takes its place.
  const current = orphans[0] ?? null;

  return (
    <OrphanCheckpointDialog
      orphans={current ? [current] : []}
      onDismiss={() => {
        if (current) handleActioned(current.meeting_folder);
        else handleDismissAll();
      }}
      onAction={handleActioned}
    />
  );
}
