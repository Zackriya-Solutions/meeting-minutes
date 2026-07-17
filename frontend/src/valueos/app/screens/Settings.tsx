'use client';
// VALUEOS: Settings — exactly three things (UI_GUIDE §8): transcript storage location (same
// setting the first-run Storage screen configures), software version, and account + Log out.
// No fabricated data: we show the real build identity (not an invented version number) and an
// honest updates message, and the account identity comes from the token claims when present.
import React, { useEffect, useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import { BUILD_INFO } from '../../buildInfo';
import { getAccessTokenClaims } from '../../debug/tokenClaims';
import { Avatar } from '../parts';
import { IcFolder, IcLogout, IcRefresh } from '../icons';

export function Settings({ onLogout }: { onLogout: () => void }) {
  const { config } = useValueOs();
  const [folder, setFolder] = useState('');
  const [saved, setSaved] = useState<'idle' | 'saved' | 'error'>('idle');
  const [account, setAccount] = useState<string | null>(null);
  const [updateMsg, setUpdateMsg] = useState('');
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    void config.getTranscriptFolder().then((f) => f && setFolder(f));
    void getAccessTokenClaims()
      .then((c) => setAccount(c?.username ?? null))
      .catch(() => setAccount(null));
  }, [config]);

  const pick = async () => {
    const picked = await config.pickFolder();
    if (picked) {
      setFolder(picked);
      await save(picked);
    }
  };

  const save = async (path: string) => {
    setSaved('idle');
    try {
      const ok = await config.validateWritable(path);
      if (!ok) {
        setSaved('error');
        return;
      }
      await config.setTranscriptFolder(path);
      setSaved('saved');
    } catch {
      setSaved('error');
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    setUpdateMsg('Checking for updates…');
    // No auto-updater is wired yet; report honestly rather than claim "up to date".
    await new Promise((r) => setTimeout(r, 700));
    setChecking(false);
    setUpdateMsg('Automatic updates aren’t available in this build yet — update by installing the latest release.');
  };

  return (
    <div className="va-page" data-testid="valueos-settings" style={{ maxWidth: 720 }}>
      <div className="va-ovl">ValueOS Agent</div>
      <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 32, margin: '4px 0 26px' }}>Settings</h1>

      {/* storage */}
      <Card title="Transcript storage location" desc="Where transcripts are written on this device before upload.">
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            className="va-input"
            data-testid="valueos-settings-folder"
            value={folder}
            onChange={(e) => {
              setFolder(e.target.value);
              setSaved('idle');
            }}
            onBlur={() => folder.trim() && save(folder)}
            placeholder="/Users/you/ValueOS Transcripts"
          />
          <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-settings-pick" onClick={pick}>
            <IcFolder size={15} /> Change folder…
          </button>
        </div>
        {saved === 'saved' && <p style={{ color: 'var(--va-signal-green)', fontSize: 13, margin: '10px 0 0' }}>Saved.</p>}
        {saved === 'error' && <p style={{ color: 'var(--va-signal-red)', fontSize: 13, margin: '10px 0 0' }}>That folder isn’t writable.</p>}
      </Card>

      {/* updates */}
      <Card title="Software updates" desc={`ValueOS Agent · ${BUILD_INFO.label}`}>
        <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-settings-update" onClick={checkUpdates} disabled={checking}>
          <IcRefresh size={15} /> Check for updates
        </button>
        {updateMsg && <p className="va-muted" data-testid="valueos-settings-update-status" style={{ fontSize: 13, margin: '10px 0 0' }}>{updateMsg}</p>}
      </Card>

      {/* account */}
      <Card title="Account" desc="">
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <Avatar name={account ?? 'ValueOS'} size={40} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 700 }}>{account ?? 'Signed in'}</div>
            <div className="va-muted" style={{ fontSize: 13 }}>Signed in to ValueOS</div>
          </div>
          <button className="va-btn va-btn-danger-outline va-btn-sm" data-testid="valueos-settings-logout" onClick={onLogout}>
            <IcLogout size={15} /> Log out
          </button>
        </div>
      </Card>
    </div>
  );
}

function Card({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div className="va-card" style={{ padding: 20, marginBottom: 16 }}>
      <h2 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 18, margin: 0 }}>{title}</h2>
      {desc && <p className="va-muted" style={{ fontSize: 13.5, margin: '4px 0 14px' }}>{desc}</p>}
      {!desc && <div style={{ height: 14 }} />}
      {children}
    </div>
  );
}
