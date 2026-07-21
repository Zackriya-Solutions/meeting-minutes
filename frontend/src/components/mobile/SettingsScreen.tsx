'use client';

import { useCallback, useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import type { VmAccent, VmModel, VmProvider, VmTheme } from './types';
import { VM_ACCENTS, VM_PROVIDER_NAMES } from './types';
import {
  getModelConfig,
  saveModelConfig,
  setLanguagePreference,
  VmModelConfig,
} from './tauriBridge';

// Baked in at CI build time (see .github/workflows/build-android.yml) so
// test builds installed side-by-side can be told apart; empty for local dev
// builds where this env var isn't set.
const BUILD_ID = process.env.NEXT_PUBLIC_BUILD_ID || '';

const LANGUAGES = ['English (auto-detect)', 'English', 'Spanish', 'French'];
const LS_LANGUAGE = 'vm-language';
const LS_NOTIFY = 'vm-notify';

// UI provider ids → backend provider ids used by the summary pipeline
const PROVIDER_TO_BACKEND: Record<VmProvider, string> = {
  ondevice: 'builtin-ai',
  ollama: 'ollama',
  claude: 'claude',
  groq: 'groq',
  openrouter: 'openrouter',
};

function backendToProvider(backend?: string): VmProvider {
  const entry = Object.entries(PROVIDER_TO_BACKEND).find(([, v]) => v === backend);
  return (entry?.[0] as VmProvider) ?? 'ondevice';
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <span
      className="muted fw8 fs11"
      style={{ display: 'block', padding: '16px 20px 4px', letterSpacing: '0.05em' }}
    >
      {children}
    </span>
  );
}

export function SettingsScreen({
  theme,
  accent,
  models,
  onSetTheme,
  onSetAccent,
  onOpenModels,
}: {
  theme: VmTheme;
  accent: VmAccent;
  models: VmModel[];
  onSetTheme: (t: VmTheme) => void;
  onSetAccent: (a: VmAccent) => void;
  onOpenModels: () => void;
}) {
  const [language, setLanguage] = useState(LANGUAGES[0]);
  const [provider, setProvider] = useState<VmProvider>('ondevice');
  const [config, setConfig] = useState<VmModelConfig | null>(null);
  const [ollamaUrl, setOllamaUrl] = useState('http://192.168.1.42:11434');
  const [apiKey, setApiKey] = useState('');
  const [notify, setNotify] = useState(true);
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'failed'>('idle');
  const [appVersion, setAppVersion] = useState('');

  useEffect(() => {
    try {
      const l = window.localStorage.getItem(LS_LANGUAGE);
      if (l) setLanguage(l);
      setNotify(window.localStorage.getItem(LS_NOTIFY) !== '0');
    } catch {
      /* ignore */
    }
    getVersion().then(setAppVersion).catch(() => {});
    getModelConfig().then((c) => {
      if (!c) return;
      setConfig(c);
      setProvider(backendToProvider(c.provider));
      if (c.ollamaEndpoint) setOllamaUrl(c.ollamaEndpoint);
      if (c.apiKey) setApiKey(c.apiKey);
    });
  }, []);

  const cycleLanguage = useCallback(() => {
    setLanguage((cur) => {
      const next = LANGUAGES[(LANGUAGES.indexOf(cur) + 1) % LANGUAGES.length];
      try {
        window.localStorage.setItem(LS_LANGUAGE, next);
      } catch {
        /* ignore */
      }
      setLanguagePreference(next.toLowerCase().includes('auto') ? 'auto' : next.toLowerCase());
      return next;
    });
  }, []);

  const persistProvider = useCallback(
    async (p: VmProvider, url: string, key: string) => {
      setSaveState('saving');
      const ok = await saveModelConfig({
        provider: PROVIDER_TO_BACKEND[p],
        model: config?.model || '',
        whisperModel: config?.whisperModel || models.find((m) => m.status === 'downloaded')?.name || 'base',
        apiKey: ['claude', 'groq', 'openrouter'].includes(p) ? key || null : null,
        ollamaEndpoint: p === 'ollama' ? url : null,
      });
      setSaveState(ok ? 'saved' : 'failed');
      setTimeout(() => setSaveState('idle'), 1800);
    },
    [config, models]
  );

  const pickProvider = useCallback(
    (p: VmProvider) => {
      setProvider(p);
      persistProvider(p, ollamaUrl, apiKey);
    },
    [persistProvider, ollamaUrl, apiKey]
  );

  const toggleNotify = useCallback(() => {
    setNotify((n) => {
      try {
        window.localStorage.setItem(LS_NOTIFY, n ? '0' : '1');
      } catch {
        /* ignore */
      }
      return !n;
    });
  }, []);

  const downloadedModel = models.find((m) => m.status === 'downloaded');

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="appbar" style={{ padding: '8px 6px 0' }}>
        <h1 style={{ paddingLeft: 10 }}>Settings</h1>
      </div>
      <div className="content" style={{ paddingBottom: 20 }}>
        <SectionLabel>APPEARANCE</SectionLabel>
        <div className="settingrow">
          <span className="f1 fs14">Theme</span>
          <div className="seg" style={{ width: 140 }}>
            <button className={theme === 'light' ? 'on' : ''} onClick={() => onSetTheme('light')}>
              Light
            </button>
            <button className={theme === 'dark' ? 'on' : ''} onClick={() => onSetTheme('dark')}>
              Dark
            </button>
          </div>
        </div>
        <div className="settingrow">
          <span className="f1 fs14">Accent color</span>
          <div className="row gap8">
            {VM_ACCENTS.map((a) => (
              <button
                key={a.id}
                className={`swatchbtn ${accent === a.id ? 'on' : ''}`}
                style={{ background: a.swatch }}
                onClick={() => onSetAccent(a.id)}
              />
            ))}
          </div>
        </div>
        <div className="divider" />

        <SectionLabel>TRANSCRIPTION</SectionLabel>
        <div className="settingrow" onClick={cycleLanguage}>
          <span className="f1 fs14">Language</span>
          <span className="muted fs13">{language}</span>
        </div>
        <div className="settingrow" onClick={onOpenModels}>
          <span className="f1 fs14">Speech model</span>
          <span className="muted fs13">
            {downloadedModel ? `${downloadedModel.name} · downloaded` : 'None downloaded'}
          </span>
        </div>
        <div className="divider" />

        <SectionLabel>SUMMARY PROVIDER</SectionLabel>
        {(Object.keys(VM_PROVIDER_NAMES) as VmProvider[]).map((p) => (
          <div key={p} className="settingrow" onClick={() => pickProvider(p)}>
            <div
              style={{
                width: 18,
                height: 18,
                borderRadius: '50%',
                border: '2px solid hsl(var(--primary))',
                flexShrink: 0,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              {provider === p && (
                <div style={{ width: 9, height: 9, borderRadius: '50%', background: 'hsl(var(--primary))' }} />
              )}
            </div>
            <span className="f1 fs14">{VM_PROVIDER_NAMES[p]}</span>
          </div>
        ))}
        {provider === 'ollama' && (
          <div style={{ padding: '6px 20px 14px' }}>
            <input
              placeholder="http://192.168.1.42:11434"
              value={ollamaUrl}
              onChange={(e) => setOllamaUrl(e.target.value)}
              onBlur={() => persistProvider(provider, ollamaUrl, apiKey)}
            />
          </div>
        )}
        {['claude', 'groq', 'openrouter'].includes(provider) && (
          <div style={{ padding: '6px 20px 14px' }}>
            <input
              type="password"
              placeholder="API key"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              onBlur={() => persistProvider(provider, ollamaUrl, apiKey)}
            />
          </div>
        )}
        {saveState !== 'idle' && (
          <div style={{ padding: '0 20px 16px' }} className="row gap10">
            <span
              className="pill"
              style={
                saveState === 'failed'
                  ? { background: 'hsl(var(--destructive)/0.15)', color: 'hsl(var(--destructive))' }
                  : { background: 'hsl(var(--accent))', color: 'hsl(var(--accent-fg))' }
              }
            >
              {saveState === 'saving' ? 'Saving…' : saveState === 'saved' ? 'Saved' : 'Save failed'}
            </span>
          </div>
        )}
        <div className="divider" />

        <SectionLabel>NOTIFICATIONS</SectionLabel>
        <div className="settingrow">
          <span className="f1 fs14">Notify when summary is ready</span>
          <button className={`switch ${notify ? 'on' : 'off'}`} onClick={toggleNotify}>
            <div className="knob" />
          </button>
        </div>
        <div className="divider" />

        <SectionLabel>PRIVACY</SectionLabel>
        <div className="settingrow">
          <div className="col f1 gap2">
            <span className="fs14">Local-only processing</span>
            <span className="muted fs12">
              Recordings and transcripts never leave this device unless you pick a cloud summary
              provider.
            </span>
          </div>
        </div>
        <div className="settingrow">
          <a
            className="f1 fs14"
            href="https://github.com/Zackriya-Solutions/meeting-minutes"
            target="_blank"
            rel="noreferrer"
          >
            View source on GitHub
          </a>
        </div>
        <div className="col ac" style={{ padding: '24px 0 10px' }}>
          <span className="muted fs12">Meetily mobile · open source</span>
          <span className="muted fs12">Your meetings never leave this device</span>
          <span className="muted mono fs11" style={{ marginTop: 6 }}>
            {appVersion && `v${appVersion}`}
            {BUILD_ID && ` · build ${BUILD_ID}`}
          </span>
        </div>
      </div>
    </div>
  );
}
