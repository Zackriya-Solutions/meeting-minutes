'use client';

import { useCallback, useEffect, useState } from 'react';
import { FileAudio, RefreshCw } from 'lucide-react';
import { fetchLocalRecordings, fetchRecordingsDiskUsage, VmLocalRecording } from './tauriBridge';

function formatDateLine(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return (
    d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }) +
    ' · ' +
    d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  );
}

function formatDuration(seconds?: number): string {
  if (!seconds || seconds <= 0) return '';
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return `${m}m ${s}s`;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 MB';
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function RecordingsScreen({ onOpenRecording }: { onOpenRecording: (rec: VmLocalRecording) => void }) {
  const [recordings, setRecordings] = useState<VmLocalRecording[]>([]);
  const [loading, setLoading] = useState(true);
  const [diskUsage, setDiskUsage] = useState<{ totalBytes: number; recordingCount: number } | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    const [recs, usage] = await Promise.all([fetchLocalRecordings(), fetchRecordingsDiskUsage()]);
    setRecordings(recs);
    setDiskUsage(usage);
    setLoading(false);
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="col f1" style={{ height: '100%', overflow: 'hidden' }}>
      <div className="appbar" style={{ padding: '10px 16px 4px' }}>
        <h1>Recordings</h1>
        <button className="iconbtn" onClick={load} aria-label="Refresh">
          <RefreshCw size={19} strokeWidth={2} />
        </button>
      </div>
      <p className="muted fs12" style={{ padding: '0 20px 4px' }}>
        Files saved on this device, straight from disk — separate from the Meetings list.
      </p>
      {diskUsage && diskUsage.recordingCount > 0 && (
        <p className="muted fs11 mono" style={{ padding: '0 20px 10px' }}>
          {diskUsage.recordingCount} recording{diskUsage.recordingCount === 1 ? '' : 's'} · {formatBytes(diskUsage.totalBytes)} used
        </p>
      )}

      <div className="content content--above-fab">
        {loading && (
          <div className="col ac jc" style={{ padding: '60px 12px' }}>
            <p className="muted fs13">Loading…</p>
          </div>
        )}

        {!loading && recordings.length === 0 && (
          <div className="col ac jc" style={{ padding: '60px 32px', textAlign: 'center', gap: 12 }}>
            <FileAudio size={40} color="hsl(var(--muted-fg, var(--fg)))" strokeWidth={1.5} />
            <h2 style={{ fontSize: 17, fontWeight: 800, margin: 0 }}>No recordings on disk yet</h2>
            <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5, maxWidth: 260 }}>
              Once you record a meeting, its files will show up here.
            </p>
          </div>
        )}

        {recordings.map((rec) => (
          <div
            key={rec.folderName}
            className="row gap12"
            style={{ padding: '14px 20px', cursor: 'pointer', borderBottom: '1px solid hsl(var(--border))' }}
            onClick={() => onOpenRecording(rec)}
          >
            <div
              style={{
                width: 40,
                height: 40,
                borderRadius: 10,
                background: 'hsl(var(--accent))',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <FileAudio size={17} color="hsl(var(--primary))" />
            </div>
            <div className="col f1" style={{ minWidth: 0, gap: 2 }}>
              <span
                style={{
                  fontWeight: 700,
                  fontSize: 14.5,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {rec.title}
              </span>
              <span className="muted mono fs11">
                {formatDateLine(rec.createdAt)}
                {rec.durationSeconds ? ` · ${formatDuration(rec.durationSeconds)}` : ''}
                {rec.status === 'recording' ? ' · in progress' : ''}
              </span>
            </div>
            <div className="row gap6">
              {rec.hasAudio && (
                <span className="chip" style={{ fontSize: 11, padding: '3px 8px' }}>
                  audio
                </span>
              )}
              {rec.hasTranscript && (
                <span className="chip" style={{ fontSize: 11, padding: '3px 8px' }}>
                  transcript
                </span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
