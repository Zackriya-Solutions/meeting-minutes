'use client';

import { useMemo, useState } from 'react';
import { Mic, Search, Upload } from 'lucide-react';
import type { VmMeeting } from './types';

function formatDateLine(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }) +
    ' · ' +
    d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

export function HomeScreen({
  meetings,
  hasModel,
  onOpenMeeting,
  onStartRecording,
  onOpenModels,
  onOpenImport,
}: {
  meetings: VmMeeting[];
  hasModel: boolean;
  onOpenMeeting: (id: string) => void;
  onStartRecording: () => void;
  onOpenModels: () => void;
  onOpenImport: () => void;
}) {
  const [search, setSearch] = useState('');
  const [searchOpen, setSearchOpen] = useState(false);
  const [blockingNoModel, setBlockingNoModel] = useState(false);

  const filtered = useMemo(
    () =>
      meetings.filter(
        (m) => !search || m.title.toLowerCase().includes(search.toLowerCase())
      ),
    [meetings, search]
  );

  const tapRecord = () => {
    if (!hasModel) {
      setBlockingNoModel(true);
      return;
    }
    onStartRecording();
  };

  return (
    <div className="col f1" style={{ height: '100%', overflow: 'hidden', position: 'relative' }}>
      <div className="appbar" style={{ padding: '10px 16px 4px' }}>
        <h1>Meetings</h1>
        <button className="iconbtn" onClick={onOpenImport} aria-label="Import audio">
          <Upload size={20} strokeWidth={2} />
        </button>
        <button
          className="iconbtn"
          onClick={() => {
            setSearchOpen((o) => !o);
            if (searchOpen) setSearch('');
          }}
        >
          <Search size={20} strokeWidth={2} />
        </button>
      </div>

      {searchOpen && (
        <div style={{ padding: '0 16px 10px' }}>
          <input
            placeholder="Search meetings"
            value={search}
            autoFocus
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      )}

      <div className="content content--above-fab">
        {meetings.length === 0 && (
          <div className="col ac jc" style={{ padding: '60px 32px', textAlign: 'center', gap: 16 }}>
            <svg className="shield" viewBox="0 0 64 74" fill="none">
              <path
                d="M32 2L60 14V34C60 54 46 66 32 72C18 66 4 54 4 34V14L32 2Z"
                fill="hsl(var(--accent))"
                stroke="hsl(var(--primary))"
                strokeWidth="2.5"
              />
              <path
                d="M32 24V40M32 48H32.01"
                stroke="hsl(var(--primary))"
                strokeWidth="4"
                strokeLinecap="round"
              />
            </svg>
            <h2 style={{ fontSize: 19, fontWeight: 800, margin: 0 }}>Nothing recorded yet</h2>
            <p className="muted" style={{ fontSize: 14, lineHeight: 1.5, margin: 0, maxWidth: 260 }}>
              Voice Me transcribes and summarizes meetings locally. Nothing leaves this device —
              ever.
            </p>
            <button className="btn btnp md" style={{ marginTop: 6 }} onClick={tapRecord}>
              Record your first meeting
            </button>
          </div>
        )}

        {meetings.length > 0 && filtered.length === 0 && (
          <div className="col ac jc" style={{ padding: '60px 32px', textAlign: 'center', gap: 8 }}>
            <p className="muted" style={{ fontSize: 14 }}>
              No meetings match &quot;{search}&quot;
            </p>
          </div>
        )}

        {filtered.map((m) => (
          <div
            key={m.id}
            className="row gap12"
            style={{ padding: '14px 20px', borderBottom: '1px solid hsl(var(--border))', cursor: 'pointer' }}
            onClick={() => onOpenMeeting(m.id)}
          >
            <div
              style={{
                width: 44,
                height: 44,
                borderRadius: 12,
                background: 'hsl(var(--accent))',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Mic size={18} color="hsl(var(--primary))" />
            </div>
            <div className="col f1" style={{ minWidth: 0, gap: 2 }}>
              <span
                style={{
                  fontWeight: 700,
                  fontSize: 15,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {m.title}
              </span>
              {m.created_at && (
                <span className="muted mono" style={{ fontSize: 12.5 }}>
                  {formatDateLine(m.created_at)}
                </span>
              )}
            </div>
          </div>
        ))}
      </div>

      {meetings.length > 0 && !blockingNoModel && (
        <button className="fab" onClick={tapRecord}>
          <span
            className="recdot"
            style={{ background: '#fff', animation: 'none' }}
          />
          Record
        </button>
      )}

      {blockingNoModel && (
        <div className="sheet-backdrop" onClick={() => setBlockingNoModel(false)}>
          <div className="card col gap14 sheet" onClick={(e) => e.stopPropagation()}>
            <div className="sheet-grip" />
            <h2 style={{ margin: 0, fontSize: 17, fontWeight: 800 }}>
              Download a speech model first
            </h2>
            <p className="muted" style={{ margin: 0, fontSize: 13.5, lineHeight: 1.5 }}>
              Voice Me needs a Whisper model on this device before it can transcribe. This only
              happens once.
            </p>
            <button
              className="btn btnp lg"
              onClick={() => {
                setBlockingNoModel(false);
                onOpenModels();
              }}
            >
              Open Model Manager
            </button>
            <button className="btn btns md" onClick={() => setBlockingNoModel(false)}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
