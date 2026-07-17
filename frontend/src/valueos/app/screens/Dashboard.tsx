'use client';
// VALUEOS: Dashboard — headline stats, the on-air banner (only truthful cross-cutting live
// state), and recent transcripts. Numbers are derived from LOCAL history only (ValueOS is
// write-only; we never read transcripts back), so every stat here is something we can
// honestly compute on-device — no "hours recorded" we don't measure.
import React from 'react';
import type { TranscriptRecord } from '../../history/transcriptHistory';
import type { ActiveCall } from '../types';
import { StatusPill, statusFromUpload } from '../parts';
import { IcPlus, IcMic } from '../icons';
import { relTime, wordCount } from '../format';

export function Dashboard({
  records,
  activeCall,
  onNew,
  onOpenRecording,
  onOpenTranscript,
}: {
  records: TranscriptRecord[];
  activeCall: ActiveCall | null;
  onNew: () => void;
  onOpenRecording: () => void;
  onOpenTranscript: (id: string) => void;
}) {
  const total = records.length;
  const words = records.reduce((n, r) => n + wordCount(r.transcript ?? ''), 0);
  const synced = records.filter((r) => r.uploadStatus === 'uploaded').length;
  const now = new Date();
  const thisMonth = records.filter((r) => {
    const d = new Date(r.createdAt);
    return d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth();
  }).length;
  const recent = records.slice(0, 6);

  return (
    <div className="va-page" data-testid="valueos-dashboard">
      <div className="va-page-head">
        <div>
          <div className="va-ovl">ValueOS Agent</div>
          <h1>Dashboard</h1>
        </div>
        <button className="va-btn va-btn-primary" data-testid="valueos-new" onClick={onNew}>
          <IcPlus size={16} /> New transcript
        </button>
      </div>

      {activeCall && (
        <button
          className="va-card va-card-hover"
          data-testid="valueos-onair-banner"
          onClick={onOpenRecording}
          style={{
            width: '100%',
            textAlign: 'left',
            display: 'flex',
            alignItems: 'center',
            gap: 14,
            padding: '16px 18px',
            marginBottom: 22,
            borderColor: 'rgba(206,54,68,.35)',
          }}
        >
          <span
            style={{
              width: 40,
              height: 40,
              borderRadius: 10,
              background: 'rgba(206,54,68,.10)',
              color: 'var(--va-signal-red)',
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              flex: '0 0 auto',
            }}
          >
            <IcMic size={20} />
          </span>
          <span style={{ minWidth: 0 }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <StatusPill status="onair" />
              <strong style={{ fontFamily: 'var(--font-display)' }}>Recording now</strong>
            </span>
            <span className="va-muted" style={{ fontSize: 13.5 }}>
              {activeCall.meta.callName} · {activeCall.meta.targetLabel} — tap to return
            </span>
          </span>
        </button>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))', gap: 14 }}>
        <StatCard label="Transcripts captured" value={total} />
        <StatCard label="Words captured" value={words} />
        <StatCard label="Uploaded to ValueOS" value={synced} />
      </div>

      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', margin: '34px 0 12px' }}>
        <div className="va-ovl">Recent transcripts</div>
        <span className="va-muted" style={{ fontSize: 13 }}>This month · {thisMonth}</span>
      </div>

      {recent.length === 0 ? (
        <div className="va-card" data-testid="valueos-dashboard-empty" style={{ padding: '28px 20px', textAlign: 'center' }}>
          <p className="va-body" style={{ margin: 0 }}>No transcripts yet.</p>
          <button className="va-btn va-btn-primary" style={{ marginTop: 14 }} onClick={onNew}>
            <IcPlus size={16} /> New transcript
          </button>
        </div>
      ) : (
        <div className="va-card" data-testid="valueos-dashboard-recent" style={{ overflow: 'hidden' }}>
          {recent.map((r, i) => (
            <button
              key={r.id}
              onClick={() => onOpenTranscript(r.id)}
              style={{
                width: '100%',
                textAlign: 'left',
                display: 'flex',
                alignItems: 'center',
                gap: 12,
                padding: '13px 16px',
                background: 'transparent',
                border: 'none',
                borderTop: i === 0 ? 'none' : '1px solid var(--va-gray-200)',
              }}
            >
              <span style={{ minWidth: 0, flex: 1 }}>
                <span style={{ display: 'block', fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  {r.targetLabel}
                </span>
                <span className="va-muted" style={{ fontSize: 12.5 }}>{relTime(r.createdAt)}</span>
              </span>
              <StatusPill status={statusFromUpload(r.uploadStatus)} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="va-card" style={{ padding: '18px 18px 20px' }}>
      <div className="va-ovl" style={{ marginBottom: 12 }}>{label}</div>
      <div className="va-stat">{value.toLocaleString()}</div>
    </div>
  );
}
