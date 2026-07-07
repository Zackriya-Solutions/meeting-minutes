'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, KeyRound, Loader2, AlertTriangle, ChevronDown, ChevronRight } from 'lucide-react';

type Settings = Record<string, string>;

const has = (s: Settings, k: string) => !!s[k] && s[k].length > 0;

export function ProviderSettings() {
  const [settings, setSettings] = useState<Settings>({});
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // DeepSeek
  const [dsKey, setDsKey] = useState('');
  const [dsModel, setDsModel] = useState('');
  // GigaChat
  const [gcAuthKey, setGcAuthKey] = useState('');
  const [gcModel, setGcModel] = useState('');
  const [gcUser, setGcUser] = useState('');
  const [gcPassword, setGcPassword] = useState('');
  const [showGcLogin, setShowGcLogin] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<Settings>('get_app_settings');
      setSettings(s || {});
      // Prefill non-secret fields; secrets stay blank (write-only).
      setDsModel(s['deepseek.model'] ?? '');
      setGcModel(s['gigachat.model'] ?? '');
      setGcUser(s['gigachat.user'] ?? '');
      if (has(s, 'gigachat.user')) setShowGcLogin(true);
    } catch {
      setSettings({});
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const deepseekConfigured = has(settings, 'deepseek.api_key');
  const gigachatConfigured =
    has(settings, 'gigachat.auth_key') || (has(settings, 'gigachat.user') && has(settings, 'gigachat.password'));

  const save = useCallback(async () => {
    setError(null);
    setSaved(false);

    const updates: [string, string][] = [];
    if (dsKey.trim()) updates.push(['deepseek.api_key', dsKey.trim()]);
    if (dsModel.trim()) updates.push(['deepseek.model', dsModel.trim()]);
    if (gcAuthKey.trim()) updates.push(['gigachat.auth_key', gcAuthKey.trim()]);
    if (gcModel.trim()) updates.push(['gigachat.model', gcModel.trim()]);
    if (gcUser.trim()) updates.push(['gigachat.user', gcUser.trim()]);
    if (gcPassword.trim()) updates.push(['gigachat.password', gcPassword.trim()]);

    if (updates.length === 0) {
      setError('Nothing to save — enter a key or value first.');
      return;
    }

    setSaving(true);
    try {
      for (const [key, value] of updates) {
        await invoke('set_app_setting', { key, value });
      }
      setDsKey('');
      setGcAuthKey('');
      setGcPassword('');
      await refresh();
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Failed to save settings.');
    } finally {
      setSaving(false);
    }
  }, [dsKey, dsModel, gcAuthKey, gcModel, gcUser, gcPassword, refresh]);

  if (!loaded) {
    return (
      <div className="mt-6 flex items-center gap-2 text-sm text-gray-400">
        <Loader2 className="h-4 w-4 animate-spin" /> Loading…
      </div>
    );
  }

  return (
    <div className="mt-6 max-w-2xl space-y-5">
      <p className="text-sm text-gray-500">
        Credentials for the Russian-market LLM providers used by Chat, summaries, and extraction. Stored locally;
        changes take effect immediately (no restart). Keys are write-only here — a configured provider shows a badge,
        and you only re-enter a key to change it.
      </p>

      {/* DeepSeek */}
      <ProviderCard
        title="DeepSeek"
        subtitle="OpenAI-compatible · used for cross-meeting synthesis"
        configured={deepseekConfigured}
      >
        <Field label="API key">
          <input
            type="password"
            value={dsKey}
            onChange={(e) => setDsKey(e.target.value)}
            placeholder={deepseekConfigured ? '•••••••• (saved — enter to replace)' : 'sk-…'}
            className={inputCls}
            autoComplete="off"
          />
        </Field>
        <Field label="Model (optional)">
          <input
            type="text"
            value={dsModel}
            onChange={(e) => setDsModel(e.target.value)}
            placeholder="deepseek-chat"
            className={inputCls}
          />
        </Field>
      </ProviderCard>

      {/* GigaChat */}
      <ProviderCard
        title="GigaChat"
        subtitle="Sber · used for fast single-meeting / lookup answers"
        configured={gigachatConfigured}
      >
        <Field label="Authorization key">
          <input
            type="password"
            value={gcAuthKey}
            onChange={(e) => setGcAuthKey(e.target.value)}
            placeholder={
              has(settings, 'gigachat.auth_key') ? '•••••••• (saved — enter to replace)' : 'base64(client_id:secret)'
            }
            className={inputCls}
            autoComplete="off"
          />
          <p className="mt-1 text-xs text-gray-400">
            The Sber “Authorization Key” (base64 of ClientID:ClientSecret) from your GigaChat project.
          </p>
        </Field>
        <Field label="Model (optional)">
          <input
            type="text"
            value={gcModel}
            onChange={(e) => setGcModel(e.target.value)}
            placeholder="GigaChat-3-Ultra"
            className={inputCls}
          />
        </Field>

        <button
          type="button"
          onClick={() => setShowGcLogin((v) => !v)}
          className="mt-1 flex items-center gap-1 text-xs text-gray-500 hover:text-gray-700"
        >
          {showGcLogin ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          Or use login &amp; password instead
        </button>
        {showGcLogin && (
          <div className="mt-2 space-y-3 border-l-2 border-gray-100 pl-3">
            <Field label="User">
              <input
                type="text"
                value={gcUser}
                onChange={(e) => setGcUser(e.target.value)}
                className={inputCls}
                autoComplete="off"
              />
            </Field>
            <Field label="Password">
              <input
                type="password"
                value={gcPassword}
                onChange={(e) => setGcPassword(e.target.value)}
                placeholder={has(settings, 'gigachat.password') ? '•••••••• (saved)' : ''}
                className={inputCls}
                autoComplete="off"
              />
            </Field>
          </div>
        )}
      </ProviderCard>

      <div className="flex items-center gap-3">
        <button
          onClick={save}
          disabled={saving}
          className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300"
        >
          {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <KeyRound className="h-4 w-4" />}
          Save credentials
        </button>
        {saved && (
          <span className="flex items-center gap-1.5 text-sm text-green-600">
            <CheckCircle2 className="h-4 w-4" /> Saved
          </span>
        )}
        {error && (
          <span className="flex items-center gap-1.5 text-sm text-red-600">
            <AlertTriangle className="h-4 w-4" /> {error}
          </span>
        )}
      </div>

      <p className="text-xs text-gray-400">
        Routing: single-meeting / short questions → GigaChat; cross-meeting synthesis &amp; extraction → DeepSeek.
        If only one provider is configured, it handles everything.
      </p>
    </div>
  );
}

const inputCls =
  'w-full rounded-lg border border-gray-200 px-3 py-2 text-sm text-gray-800 focus:border-blue-400 focus:outline-none';

function ProviderCard({
  title,
  subtitle,
  configured,
  children,
}: {
  title: string;
  subtitle: string;
  configured: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-gray-200 p-5">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-gray-900">{title}</h3>
          <p className="text-xs text-gray-400">{subtitle}</p>
        </div>
        {configured ? (
          <span className="flex items-center gap-1 rounded-full bg-green-50 px-2 py-0.5 text-xs font-medium text-green-700">
            <CheckCircle2 className="h-3.5 w-3.5" /> Configured
          </span>
        ) : (
          <span className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-500">Not set</span>
        )}
      </div>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-gray-600">{label}</span>
      {children}
    </label>
  );
}
