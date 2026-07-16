import React, { useEffect, useState } from 'react';
import type { CSSProperties } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import type { TranscriptRecord } from '../../history/transcriptHistory';
import { VaLogo, VA_BLUE } from '../../assets/VaLogo';

// VALUEOS: home / hub — a two-pane master-detail (like a mail client): the LEFT pane lists
// the locally-captured transcripts; clicking one loads its DETAIL on the RIGHT (tenant,
// lead/opportunity, digest, and the transcript itself). A prominent "New transcript" button
// launches the tenant→type→target wizard. ValueOS is write-only, so this is the LOCAL
// capture history on this device.
function statusLabel(s: TranscriptRecord['uploadStatus']): string {
  return s === 'uploaded' ? 'Uploaded' : s === 'failed' ? 'Not uploaded' : 'Pending upload';
}

export function HomeScreen({ onNew }: { onNew: () => void }) {
  const { history } = useValueOs();
  const [records, setRecords] = useState<TranscriptRecord[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    history.list().then((r) => {
      setRecords(r);
      setSelectedId((cur) => cur ?? r[0]?.id ?? null);
    });
  }, [history]);

  const selected = records.find((r) => r.id === selectedId) ?? null;

  return (
    <div data-testid="valueos-home" style={styles.root}>
      <header style={styles.header}>
        <div style={styles.brand}>
          <VaLogo size={30} background={false} />
          <span style={styles.brandText}>ValueOS Agent</span>
        </div>
        <button data-testid="valueos-home-new" style={styles.newBtn} onClick={onNew}>
          + New transcript
        </button>
      </header>

      <div style={styles.body}>
        {/* LEFT — transcript list */}
        <aside style={styles.listPane}>
          <h2 style={styles.listTitle}>Your transcripts</h2>
          {records.length === 0 ? (
            <p data-testid="valueos-home-empty" style={styles.empty}>
              No transcripts yet — start one with “New transcript”.
            </p>
          ) : (
            <ul data-testid="valueos-home-list" style={styles.list}>
              {records.map((r) => (
                <li key={r.id} style={{ listStyle: 'none' }}>
                  <button
                    data-testid={`valueos-home-item-${r.id}`}
                    style={r.id === selectedId ? styles.itemOn : styles.item}
                    onClick={() => setSelectedId(r.id)}
                  >
                    <span style={styles.itemLabel}>{r.targetLabel}</span>
                    <span style={styles.itemMeta}>
                      {new Date(r.createdAt).toLocaleString()} · {r.activityType}
                    </span>
                    <span style={pill(r.uploadStatus)}>{statusLabel(r.uploadStatus)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        {/* RIGHT — detail of the selected transcript */}
        <section data-testid="valueos-home-detail" style={styles.detailPane}>
          {!selected ? (
            <div style={styles.detailEmpty}>Select a transcript on the left to see its details.</div>
          ) : (
            <div style={styles.detailInner}>
              <h1 style={styles.detailTitle}>{selected.targetLabel}</h1>
              <div style={styles.metaGrid}>
                <Meta k="Tenant" v={selected.tenantName ?? selected.tenantId} />
                <Meta k="Type" v={selected.activityType === 'lead' ? 'Lead' : 'Opportunity'} />
                <Meta k="Attached to" v={selected.targetLabel} />
                <Meta k="Captured" v={new Date(selected.createdAt).toLocaleString()} />
                <Meta k="Upload" v={statusLabel(selected.uploadStatus)} />
              </div>

              <h3 style={styles.sectionTitle}>Digest</h3>
              <div style={styles.digestBox}>
                {selected.digest || 'No digest stored for this transcript.'}
              </div>

              <h3 style={styles.sectionTitle}>Transcript</h3>
              <pre style={styles.transcriptBox}>
                {selected.transcript || `Transcript text not stored locally.\nSaved to: ${selected.path}`}
              </pre>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function Meta({ k, v }: { k: string; v: string }) {
  return (
    <div style={styles.metaCell}>
      <div style={styles.metaKey}>{k}</div>
      <div style={styles.metaVal}>{v}</div>
    </div>
  );
}

function pill(s: TranscriptRecord['uploadStatus']): CSSProperties {
  const base: CSSProperties = {
    fontSize: 11,
    fontWeight: 700,
    padding: '2px 8px',
    borderRadius: 999,
    alignSelf: 'flex-start',
    marginTop: 6,
  };
  if (s === 'uploaded') return { ...base, background: '#fff', color: VA_BLUE };
  if (s === 'failed')
    return { ...base, background: 'rgba(255,215,215,0.15)', color: '#ffd7d7', border: '1px solid rgba(255,215,215,0.5)' };
  return { ...base, background: 'rgba(255,255,255,0.15)', color: '#fff', border: '1px solid rgba(255,255,255,0.4)' };
}

const styles: Record<string, CSSProperties> = {
  root: {
    position: 'fixed',
    inset: 0,
    background: VA_BLUE,
    color: '#ffffff',
    fontFamily: 'system-ui, -apple-system, "Segoe UI", sans-serif',
    display: 'flex',
    flexDirection: 'column',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '16px 24px',
    borderBottom: '1px solid rgba(255,255,255,0.15)',
    flex: '0 0 auto',
  },
  brand: { display: 'flex', alignItems: 'center', gap: 10 },
  brandText: { fontSize: 18, fontWeight: 800 },
  newBtn: {
    background: '#ffffff',
    color: VA_BLUE,
    border: 'none',
    borderRadius: 10,
    padding: '10px 18px',
    fontSize: 14,
    fontWeight: 700,
    cursor: 'pointer',
  },
  body: { display: 'flex', flex: 1, minHeight: 0 },
  listPane: {
    width: 340,
    flex: '0 0 auto',
    borderRight: '1px solid rgba(255,255,255,0.15)',
    overflowY: 'auto',
    padding: '16px 12px',
  },
  listTitle: {
    fontSize: 12,
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
    opacity: 0.7,
    margin: '4px 8px 12px',
  },
  empty: { fontSize: 14, opacity: 0.8, padding: '0 8px', lineHeight: 1.5 },
  list: { listStyle: 'none', padding: 0, margin: 0 },
  item: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'flex-start',
    width: '100%',
    textAlign: 'left',
    background: 'transparent',
    border: 'none',
    borderLeft: '3px solid transparent',
    borderRadius: 8,
    padding: '10px 12px',
    color: '#ffffff',
    cursor: 'pointer',
    marginBottom: 2,
  },
  itemOn: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'flex-start',
    width: '100%',
    textAlign: 'left',
    background: 'rgba(255,255,255,0.16)',
    border: 'none',
    borderLeft: '3px solid #ffffff',
    borderRadius: 8,
    padding: '10px 12px',
    color: '#ffffff',
    cursor: 'pointer',
    marginBottom: 2,
  },
  itemLabel: { fontSize: 15, fontWeight: 700 },
  itemMeta: { fontSize: 12, opacity: 0.75, marginTop: 2 },
  detailPane: { flex: 1, minWidth: 0, overflowY: 'auto', padding: '24px 28px' },
  detailEmpty: { opacity: 0.7, fontSize: 15, marginTop: 40, textAlign: 'center' },
  detailInner: { maxWidth: 780 },
  detailTitle: { fontSize: 26, fontWeight: 800, margin: '0 0 16px' },
  metaGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))',
    gap: 12,
    marginBottom: 8,
  },
  metaCell: { background: 'rgba(255,255,255,0.08)', borderRadius: 10, padding: '10px 12px' },
  metaKey: { fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.5, opacity: 0.65, fontWeight: 700 },
  metaVal: { fontSize: 15, fontWeight: 600, marginTop: 3, wordBreak: 'break-word' },
  sectionTitle: {
    fontSize: 12,
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
    opacity: 0.7,
    margin: '20px 0 8px',
  },
  digestBox: {
    background: 'rgba(255,255,255,0.1)',
    borderRadius: 10,
    padding: '14px 16px',
    fontSize: 14,
    lineHeight: 1.55,
    whiteSpace: 'pre-wrap',
  },
  transcriptBox: {
    background: 'rgba(0,0,0,0.18)',
    borderRadius: 10,
    padding: '14px 16px',
    fontSize: 13,
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    margin: 0,
    maxHeight: 360,
    overflowY: 'auto',
  },
};
