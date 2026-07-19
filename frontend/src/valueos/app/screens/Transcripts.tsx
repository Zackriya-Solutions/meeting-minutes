'use client';
// VALUEOS: Transcripts — a fixed-width list (on-air row first, then saved transcripts) beside
// a reader. Reader order follows UI_GUIDE §6: status → title/meta → filing card → sync bar →
// digest (placed ABOVE the transcript because it's shorter) → full transcript. There is no
// "Open in ValueOS" deep link and no participant/platform metadata (both out of scope). The
// live/on-air call opens the Recording screen, not the reader.
import React from 'react';
import type { TranscriptRecord } from '../../history/transcriptHistory';
import type { ActiveCall } from '../types';
import { StatusPill, statusFromUpload, Avatar } from '../parts';
import { IcPlus, IcMic, IcCheck, IcTrash } from '../icons';
import { relTime } from '../format';

export function Transcripts({
  records,
  activeCall,
  selectedId,
  onSelect,
  onNew,
  onOpenRecording,
  onDelete,
}: {
  records: TranscriptRecord[];
  activeCall: ActiveCall | null;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onOpenRecording: () => void;
  onDelete: (id: string) => void;
}) {
  const selected = records.find((r) => r.id === selectedId) ?? null;

  return (
    <div data-testid="valueos-transcripts" style={{ display: 'flex', height: '100%', minHeight: 0 }}>
      {/* list */}
      <div
        className="va-scroll"
        style={{ width: 352, flex: '0 0 auto', borderRight: '1px solid var(--va-gray-200)', overflowY: 'auto', display: 'flex', flexDirection: 'column' }}
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '20px 18px 12px' }}>
          <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 24, margin: 0 }}>Transcripts</h1>
          <button className="va-btn va-btn-primary va-btn-sm" data-testid="valueos-new" onClick={onNew}>
            <IcPlus size={15} /> New
          </button>
        </div>

        {activeCall && (
          <button
            data-testid="valueos-transcripts-onair"
            onClick={onOpenRecording}
            style={{ ...rowStyle, background: 'rgba(206,54,68,.05)', borderLeft: '3px solid var(--va-signal-red)' }}
          >
            <span style={{ minWidth: 0, flex: 1 }}>
              <span style={rowTitle}><IcMic size={14} /> {activeCall.meta.callName}</span>
              <span className="va-muted" style={rowMeta}>{activeCall.meta.targetLabel}</span>
            </span>
            <StatusPill status="onair" />
          </button>
        )}

        {records.length === 0 && !activeCall && (
          <p className="va-muted" data-testid="valueos-transcripts-empty" style={{ padding: '20px 18px', fontSize: 14 }}>
            No transcripts yet. Start one with “New”.
          </p>
        )}

        {records.map((r) => (
          <button
            key={r.id}
            data-testid={`valueos-transcripts-row-${r.id}`}
            onClick={() => onSelect(r.id)}
            style={{ ...rowStyle, background: r.id === selectedId ? 'var(--va-gray-100)' : 'transparent' }}
          >
            <span style={{ minWidth: 0, flex: 1 }}>
              <span style={rowTitle}>{r.targetLabel}</span>
              <span className="va-muted" style={rowMeta}>{relTime(r.createdAt)}</span>
            </span>
            <StatusPill status={statusFromUpload(r.uploadStatus)} />
          </button>
        ))}
      </div>

      {/* reader */}
      <div className="va-scroll" style={{ flex: 1, minWidth: 0, overflowY: 'auto' }}>
        {selected ? <Reader record={selected} onDelete={onDelete} /> : (
          <div style={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 40 }}>
            <p className="va-muted" data-testid="valueos-transcripts-placeholder">Select a transcript to read it.</p>
          </div>
        )}
      </div>
    </div>
  );
}

function Reader({ record, onDelete }: { record: TranscriptRecord; onDelete: (id: string) => void }) {
  const status = statusFromUpload(record.uploadStatus);
  const points = digestPoints(record.digest ?? '');
  const lines = (record.transcript ?? '').split('\n').map((l) => l.trim()).filter(Boolean);

  const [confirming, setConfirming] = React.useState(false);

  return (
    <div data-testid="valueos-transcripts-reader" style={{ maxWidth: 720, margin: '0 auto', padding: '28px 32px 56px' }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12 }}>
        <div style={{ minWidth: 0 }}>
          <StatusPill status={status} />
          <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 30, margin: '12px 0 6px' }}>
            {record.targetLabel}
          </h1>
          <p className="va-muted" style={{ fontSize: 14, margin: 0 }}>
            {new Date(record.createdAt).toLocaleString()}
          </p>
        </div>
        {confirming ? (
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', flex: '0 0 auto' }}>
            <span className="va-muted" style={{ fontSize: 12.5, whiteSpace: 'nowrap' }}>
              Delete locally?{record.uploadStatus === 'uploaded' ? ' (ValueOS copy kept)' : ''}
            </span>
            <button className="va-btn va-btn-danger va-btn-sm" data-testid="valueos-transcript-delete-confirm" onClick={() => onDelete(record.id)}>
              Delete
            </button>
            <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-transcript-delete-cancel" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </div>
        ) : (
          <button
            className="va-btn va-btn-danger-outline va-btn-sm"
            data-testid="valueos-transcript-delete"
            onClick={() => setConfirming(true)}
            title="Delete this local transcript (does not affect the ValueOS cloud copy)"
            style={{ flex: '0 0 auto' }}
          >
            <IcTrash size={15} /> Delete
          </button>
        )}
      </div>

      {/* filing card */}
      <div className="va-card" style={{ marginTop: 20, padding: 16, boxShadow: 'none', display: 'flex', gap: 24, flexWrap: 'wrap' }}>
        <Field k="Tenant" v={record.tenantName ?? record.tenantId} />
        <Field k="Type" v={<span className={`va-pill ${record.activityType === 'lead' ? 'va-pill-pending' : 'va-pill-syncing'}`} style={{ textTransform: 'capitalize' }}>{record.activityType}</span>} />
        <Field k="ValueOS record" v={record.targetLabel} />
      </div>

      {/* sync bar */}
      <div
        style={{
          marginTop: 14,
          padding: '11px 14px',
          borderRadius: 10,
          fontSize: 13.5,
          background: status === 'synced' ? 'rgba(44,183,134,.08)' : status === 'failed' ? 'rgba(206,54,68,.08)' : 'var(--va-gray-100)',
          color: status === 'synced' ? 'var(--va-signal-green)' : status === 'failed' ? 'var(--va-signal-red)' : 'var(--va-gray-600)',
          fontWeight: 600,
        }}
      >
        {status === 'synced'
          ? 'Synced to ValueOS · Transcript delivered'
          : status === 'failed'
            ? `Upload failed · ${record.error ?? 'not sent'} · Saved locally`
            : status === 'pending'
              ? 'Pending · Saved locally, will upload'
              : 'Syncing to ValueOS…'}
      </div>

      {/* digest (above the transcript on purpose) */}
      {record.digest && (
        <>
          <div className="va-ovl" style={{ margin: '28px 0 10px' }}>Digest</div>
          {points.summary && <p className="va-body" style={{ margin: '0 0 12px' }}>{points.summary}</p>}
          {points.keyPoints.map((p, i) => (
            <div key={i} style={{ display: 'flex', gap: 10, alignItems: 'flex-start', margin: '7px 0' }}>
              <span style={{ color: 'var(--va-blue)', marginTop: 2, flex: '0 0 auto' }}><IcCheck size={16} /></span>
              <span className="va-body" style={{ margin: 0 }}>{p}</span>
            </div>
          ))}
        </>
      )}

      {/* transcript */}
      <div className="va-ovl" style={{ margin: '30px 0 12px' }}>Transcript</div>
      {lines.length === 0 ? (
        <p className="va-muted">No speech was captured.</p>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          {lines.map((line, i) => (
            <div key={i} style={{ display: 'flex', gap: 12 }}>
              <Avatar name="You" you />
              <div>
                <div style={{ fontWeight: 700, color: 'var(--va-blue)', fontSize: 14, marginBottom: 3 }}>You</div>
                <p className="va-body" style={{ margin: 0 }}>{line}</p>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** Split the digest recap into a lead paragraph + key points. If the digest is bullet-like
 *  (multiple lines / “- ” prefixes) the tail becomes points; otherwise it's one paragraph. */
function digestPoints(digest: string): { summary: string; keyPoints: string[] } {
  const raw = digest.split('\n').map((l) => l.replace(/^[-•*]\s*/, '').trim()).filter(Boolean);
  if (raw.length <= 1) return { summary: digest.trim(), keyPoints: [] };
  return { summary: raw[0], keyPoints: raw.slice(1) };
}

function Field({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div>
      <div className="va-ovl" style={{ marginBottom: 6 }}>{k}</div>
      <div style={{ fontWeight: 600, fontSize: 14 }}>{v}</div>
    </div>
  );
}

const rowStyle: React.CSSProperties = {
  width: '100%',
  textAlign: 'left',
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  padding: '13px 16px',
  border: 'none',
  borderTop: '1px solid var(--va-gray-200)',
  background: 'transparent',
};
const rowTitle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 6,
  fontWeight: 600,
  fontSize: 14.5,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};
const rowMeta: React.CSSProperties = { display: 'block', fontSize: 12.5, marginTop: 2 };
