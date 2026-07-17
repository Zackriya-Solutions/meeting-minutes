'use client';
// VALUEOS: small shared presentational atoms used across the redesigned screens.
import React from 'react';

/** The per-transcript cloud state. 'onair' is NOT a sync state (UI_GUIDE §3) — it means a
 *  live call that isn't a transcript yet. */
export type CallStatus = 'onair' | 'pending' | 'syncing' | 'synced' | 'failed';

export function statusFromUpload(s: 'uploaded' | 'pending' | 'failed'): CallStatus {
  return s === 'uploaded' ? 'synced' : s;
}

const LABEL: Record<CallStatus, string> = {
  onair: 'On air',
  pending: 'Pending',
  syncing: 'Syncing',
  synced: 'Synced',
  failed: 'Failed',
};

export function StatusPill({ status }: { status: CallStatus }) {
  return (
    <span className={`va-pill va-pill-${status}`} data-testid={`valueos-status-${status}`}>
      {status === 'onair' && <span className="va-dot va-dot-red" />}
      {LABEL[status]}
    </span>
  );
}

function initials(label: string): string {
  const parts = label.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

/** Speaker avatar. "You" (the local user) renders blue; everyone else neutral (UI_GUIDE §4). */
export function Avatar({ name, you = false, size = 34 }: { name: string; you?: boolean; size?: number }) {
  return (
    <span
      aria-hidden="true"
      style={{
        width: size,
        height: size,
        flex: '0 0 auto',
        borderRadius: '50%',
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: size * 0.36,
        fontWeight: 800,
        fontFamily: 'var(--font-display)',
        background: you ? 'var(--va-blue)' : 'var(--va-gray-100)',
        color: you ? '#fff' : 'var(--va-gray-600)',
      }}
    >
      {initials(name)}
    </span>
  );
}
