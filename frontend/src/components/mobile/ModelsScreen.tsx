'use client';

import { useCallback } from 'react';
import { ChevronLeft } from 'lucide-react';
import type { VmModel } from './types';
import { deleteModel, downloadModel } from './tauriBridge';

export function ModelsScreen({
  models,
  onModelsChanged,
  onBack,
}: {
  models: VmModel[];
  onModelsChanged: () => void;
  onBack: () => void;
}) {
  const downloadedMb = models
    .filter((m) => m.status === 'downloaded')
    .reduce((a, m) => a + m.size_mb, 0);
  const totalMb = models.reduce((a, m) => a + m.size_mb, 0) || 1;

  const startDownload = useCallback(
    async (name: string) => {
      try {
        await downloadModel(name);
        onModelsChanged();
      } catch (e) {
        console.warn('[vm] model download failed', e);
      }
    },
    [onModelsChanged]
  );

  const remove = useCallback(
    async (name: string) => {
      try {
        await deleteModel(name);
      } catch (e) {
        console.warn('[vm] model delete failed', e);
      }
      onModelsChanged();
    },
    [onModelsChanged]
  );

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="appbar" style={{ padding: '8px 6px 0' }}>
        <button className="iconbtn" onClick={onBack}>
          <ChevronLeft size={22} strokeWidth={2.2} />
        </button>
        <h1>Speech Models</h1>
      </div>
      <div className="content" style={{ padding: '4px 20px 16px' }}>
        <div className="card" style={{ padding: 16, marginBottom: 16 }}>
          <div className="row between" style={{ marginBottom: 8 }}>
            <span className="fw7 fs13">Storage used</span>
            <span className="mono muted fs12">
              {downloadedMb.toFixed(0)} MB of {totalMb.toFixed(0)} MB
            </span>
          </div>
          <div className="progress-track" style={{ height: 8 }}>
            <div
              className="progress-fill"
              style={{ width: `${Math.round((downloadedMb / totalMb) * 100)}%` }}
            />
          </div>
        </div>

        {models.length === 0 && (
          <p className="muted fs13" style={{ textAlign: 'center', padding: '20px 0' }}>
            Model catalog unavailable — is the Whisper engine initialized?
          </p>
        )}

        {models.map((m) => (
          <div key={m.name} className="card" style={{ padding: '14px 16px', marginBottom: 10 }}>
            <div className="row between">
              <div className="col gap2" style={{ minWidth: 0 }}>
                <div className="row gap8">
                  <span className="fw7 fs15">{m.name}</span>
                  {m.recommended && m.status !== 'downloaded' && (
                    <span
                      className="pill"
                      style={{
                        background: 'hsl(var(--accent))',
                        color: 'hsl(var(--accent-fg))',
                        fontSize: 11,
                        padding: '3px 8px',
                      }}
                    >
                      Recommended
                    </span>
                  )}
                </div>
                <span className="muted fs12">
                  {m.size_mb} MB{m.description ? ` · ${m.description}` : ''}
                </span>
              </div>
              {m.status === 'available' && (
                <button className="btn btnghost sm" onClick={() => startDownload(m.name)}>
                  Download
                </button>
              )}
              {m.status === 'downloaded' && (
                <button className="btn btns sm" onClick={() => remove(m.name)}>
                  Delete
                </button>
              )}
            </div>
            {m.status === 'downloading' && (
              <div style={{ marginTop: 10 }}>
                <div className="progress-track">
                  <div className="progress-fill" style={{ width: `${m.progress}%` }} />
                </div>
                <span className="mono muted fs11">{m.progress}%</span>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
