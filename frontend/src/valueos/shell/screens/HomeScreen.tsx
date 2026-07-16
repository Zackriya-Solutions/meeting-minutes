import React, { useEffect, useState } from 'react';
import type { CSSProperties } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import type { TranscriptRecord } from '../../history/transcriptHistory';
import { VaLogo } from '../../assets/VaLogo';
import * as ui from './ui';

// VALUEOS: home / hub — the list of locally-captured transcripts + a control to start a
// new capture (which launches the tenant→type→target wizard). ValueOS is write-only, so
// this list is the LOCAL capture history on this device.
export function HomeScreen({ onNew }: { onNew: () => void }) {
  const { history } = useValueOs();
  const [records, setRecords] = useState<TranscriptRecord[]>([]);

  useEffect(() => {
    history.list().then(setRecords);
  }, [history]);

  return (
    <div data-testid="valueos-home" style={{ ...ui.page, justifyContent: 'flex-start', padding: '48px 24px 24px' }}>
      <div style={{ width: '100%', maxWidth: 640, display: 'flex', flexDirection: 'column' }}>
        <header style={rowBetween}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <VaLogo size={40} background={false} />
            <h1 style={{ fontSize: 24, fontWeight: 800, margin: 0 }}>ValueOS Agent</h1>
          </div>
          <button data-testid="valueos-home-new" style={ui.primaryBtn} onClick={onNew}>
            New transcript
          </button>
        </header>

        <h2 style={{ fontSize: 15, fontWeight: 700, opacity: 0.85, margin: '28px 0 12px', textAlign: 'left' }}>
          Your transcripts
        </h2>

        {records.length === 0 ? (
          <p data-testid="valueos-home-empty" style={{ ...ui.sub, textAlign: 'left' }}>
            No transcripts yet — start one with “New transcript”.
          </p>
        ) : (
          <ul data-testid="valueos-home-list" style={{ listStyle: 'none', padding: 0, margin: 0 }}>
            {records.map((r) => (
              <li key={r.id} data-testid={`valueos-home-item-${r.id}`} style={item}>
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start' }}>
                  <span style={{ fontWeight: 600 }}>{r.targetLabel}</span>
                  <span style={{ fontSize: 12, opacity: 0.7 }}>
                    {new Date(r.createdAt).toLocaleString()} · {r.activityType}
                  </span>
                </div>
                <span style={r.uploadStatus === 'uploaded' ? badgeUp : badgePending}>
                  {r.uploadStatus === 'uploaded' ? 'Uploaded' : 'Pending upload'}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}

const rowBetween: CSSProperties = { display: 'flex', alignItems: 'center', justifyContent: 'space-between' };
const item: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: '12px 14px',
  borderRadius: 10,
  background: 'rgba(255,255,255,0.08)',
  marginBottom: 8,
};
const badgeUp: CSSProperties = { fontSize: 12, fontWeight: 700, padding: '4px 10px', borderRadius: 999, background: '#fff', color: '#0030BC' };
const badgePending: CSSProperties = { fontSize: 12, fontWeight: 700, padding: '4px 10px', borderRadius: 999, background: 'rgba(255,255,255,0.15)', color: '#fff', border: '1px solid rgba(255,255,255,0.4)' };
