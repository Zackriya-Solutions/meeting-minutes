'use client';
// VALUEOS: Settings — exactly three things (UI_GUIDE §8): transcript storage location (same
// setting the first-run Storage screen configures), software version, and account + Log out.
// No fabricated data: we show the real build identity (not an invented version number) and an
// honest updates message, and the account identity comes from the token claims when present.
import React, { useEffect, useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import { BUILD_INFO } from '../../buildInfo';
import { getAccessTokenClaims } from '../../debug/tokenClaims';
import type { UpdateCheckResult } from '../../api/types';
import { Avatar } from '../parts';
import { IcFolder, IcLogout, IcRefresh } from '../icons';

export function Settings({ onLogout, tenantId }: { onLogout: () => void; tenantId?: string }) {
  const { config, updater } = useValueOs();
  const [folder, setFolder] = useState('');
  const [saved, setSaved] = useState<'idle' | 'saved' | 'error'>('idle');
  const [account, setAccount] = useState<string | null>(null);
  const [updateMsg, setUpdateMsg] = useState('');
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [applying, setApplying] = useState(false);

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
    if (!tenantId) {
      setUpdate(null);
      setUpdateMsg('Sign in to a ValueOS workspace to check for updates.');
      return;
    }
    setChecking(true);
    setUpdate(null);
    setUpdateMsg('Checking for updates…');
    const out = await updater.checkForUpdates(tenantId);
    setChecking(false);
    if (out.status === 'up-to-date') {
      setUpdateMsg('You’re on the latest version.');
    } else if (out.status === 'available' && out.result?.update_available) {
      setUpdate(out.result);
      setUpdateMsg(`Update available: ${out.result.latest_version ?? 'new version'}.`);
    } else if (out.status === 'reauth') {
      setUpdateMsg(out.error ?? 'Please sign in again to check for updates.');
    } else if (out.status === 'deEntitled') {
      setUpdateMsg('This workspace no longer has ValueOS Agent access.');
    } else {
      setUpdateMsg(out.error ?? 'Could not check for updates.');
    }
  };

  const installUpdate = async () => {
    if (!tenantId || !update) return;
    setApplying(true);
    setUpdateMsg('Downloading and verifying the update…');
    // Prompt-first + notify_only: the user has explicitly confirmed by clicking install.
    const out = await updater.downloadAndApply(tenantId, update);
    setApplying(false);
    // On success the app opens the verified installer and exits; if we're still here it failed.
    setUpdateMsg(
      out.status === 'applying'
        ? 'Opening the installer… the app will close so it can update. Your data is preserved.'
        : (out.error ?? 'The update could not be applied.'),
    );
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
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <button
            className="va-btn va-btn-ghost-light va-btn-sm"
            data-testid="valueos-settings-update"
            onClick={checkUpdates}
            disabled={checking || applying}
          >
            <IcRefresh size={15} /> Check for updates
          </button>
          {update?.update_available && (
            <button
              className="va-btn va-btn-primary va-btn-sm"
              data-testid="valueos-settings-install"
              onClick={installUpdate}
              disabled={applying}
            >
              {applying ? 'Updating…' : `Download & install ${update.latest_version ?? ''}`.trim()}
            </button>
          )}
        </div>
        {update?.notes && (
          <p className="va-body" style={{ fontSize: 13.5, margin: '10px 0 0' }}>{update.notes}</p>
        )}
        {updateMsg && (
          <p className="va-muted" data-testid="valueos-settings-update-status" style={{ fontSize: 13, margin: '10px 0 0' }}>
            {updateMsg}
          </p>
        )}
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
