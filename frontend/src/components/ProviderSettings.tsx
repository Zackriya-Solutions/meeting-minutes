'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, KeyRound, Loader2, AlertTriangle, ChevronDown, ChevronRight } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

type Settings = Record<string, string>;

const has = (s: Settings, k: string) => !!s[k] && s[k].length > 0;

export function ProviderSettings() {
  const t = useT();
  return (
    <div className="mt-6 max-w-2xl rounded-[var(--radius-24)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-5">
      <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Managed cloud services')}</h3>
      <p className="mt-1 text-sm text-[var(--fg2)]">
        {t('DeepSeek is used for summaries and the knowledge base. SaluteSpeech is used for transcription and speaker detection. Access is provided through the Memento gateway; no API keys are required.')}
      </p>
    </div>
  );
}

// Kept internally so BYOK can be restored later without exposing it in the pilot UI.
function LegacyProviderSettings() {
  const t = useT();
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
      setError(t('Nothing to save — enter a key or value first.'));
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
      setError(typeof e === 'string' ? e : t('Failed to save settings.'));
    } finally {
      setSaving(false);
    }
  }, [dsKey, dsModel, gcAuthKey, gcModel, gcUser, gcPassword, refresh, t]);

  if (!loaded) {
    return (
      <div className="mt-6 flex items-center gap-2 text-sm text-[var(--fg3)]">
        <Loader2 className="h-4 w-4 animate-spin" /> {t('Loading…')}
      </div>
    );
  }

  return (
    <div className="mt-6 max-w-2xl space-y-5">
      <p className="text-sm text-[var(--fg2)]">
        {t('Credentials for the Russian-market LLM providers used by Chat, summaries, and extraction. Stored locally; changes take effect immediately (no restart). Keys are write-only here — a configured provider shows a badge, and you only re-enter a key to change it.')}
      </p>

      {/* DeepSeek */}
      <ProviderCard
        title="DeepSeek"
        subtitle={t('OpenAI-compatible · used for cross-meeting synthesis')}
        configured={deepseekConfigured}
      >
        <Field label={t('API key')}>
          <input
            type="password"
            value={dsKey}
            onChange={(e) => setDsKey(e.target.value)}
            placeholder={deepseekConfigured ? t('•••••••• (saved — enter to replace)') : 'sk-…'}
            className={inputCls}
            autoComplete="off"
          />
        </Field>
        <Field label={t('Model (optional)')}>
          <input
            type="text"
            value={dsModel}
            onChange={(e) => setDsModel(e.target.value)}
            placeholder="deepseek-v4-flash"
            className={inputCls}
          />
        </Field>
      </ProviderCard>

      {/* GigaChat */}
      <ProviderCard
        title="GigaChat"
        subtitle={t('Sber · used for fast single-meeting / lookup answers')}
        configured={gigachatConfigured}
      >
        <Field label={t('Authorization key')}>
          <input
            type="password"
            value={gcAuthKey}
            onChange={(e) => setGcAuthKey(e.target.value)}
            placeholder={
              has(settings, 'gigachat.auth_key') ? t('•••••••• (saved — enter to replace)') : 'base64(client_id:secret)'
            }
            className={inputCls}
            autoComplete="off"
          />
          <p className="mt-1 text-xs text-[var(--fg3)]">
            {t('The Sber “Authorization Key” (base64 of ClientID:ClientSecret) from your GigaChat project.')}
          </p>
        </Field>
        <Field label={t('Model (optional)')}>
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
          className="mt-1 flex items-center gap-1 text-xs text-[var(--fg2)] hover:text-[var(--fg1)]"
        >
          {showGcLogin ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          {t('Or use login & password instead')}
        </button>
        {showGcLogin && (
          <div className="mt-2 space-y-3 border-l-2 border-[var(--border-subtle)] pl-3">
            <Field label={t('User')}>
              <input
                type="text"
                value={gcUser}
                onChange={(e) => setGcUser(e.target.value)}
                className={inputCls}
                autoComplete="off"
              />
            </Field>
            <Field label={t('Password')}>
              <input
                type="password"
                value={gcPassword}
                onChange={(e) => setGcPassword(e.target.value)}
                placeholder={has(settings, 'gigachat.password') ? t('•••••••• (saved)') : ''}
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
          className="flex items-center gap-2 rounded-lg bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)] disabled:cursor-not-allowed disabled:bg-[var(--bg-elevated)]"
        >
          {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <KeyRound className="h-4 w-4" />}
          {t('Save credentials')}
        </button>
        {saved && (
          <span className="flex items-center gap-1.5 text-sm text-[var(--success)]">
            <CheckCircle2 className="h-4 w-4" /> {t('Saved')}
          </span>
        )}
        {error && (
          <span className="flex items-center gap-1.5 text-sm text-[var(--danger)]">
            <AlertTriangle className="h-4 w-4" /> {error}
          </span>
        )}
      </div>

      <p className="text-xs text-[var(--fg3)]">
        {t('Routing: single-meeting / short questions → GigaChat; cross-meeting synthesis & extraction → DeepSeek. If only one provider is configured, it handles everything.')}
      </p>
    </div>
  );
}

const inputCls =
  'w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-input)] px-3 py-2 text-sm text-[var(--fg1)] placeholder:text-[var(--fg3)] focus:border-[var(--gold-border)] focus:outline-none';

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
  const t = useT();
  return (
    <div className="rounded-xl border border-[var(--border-subtle)] p-5">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-[var(--fg1)]">{title}</h3>
          <p className="text-xs text-[var(--fg3)]">{subtitle}</p>
        </div>
        {configured ? (
          <span className="flex items-center gap-1 rounded-full bg-[color-mix(in_srgb,var(--success)_12%,transparent)] px-2 py-0.5 text-xs font-medium text-[var(--success)]">
            <CheckCircle2 className="h-3.5 w-3.5" /> {t('Configured')}
          </span>
        ) : (
          <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-xs text-[var(--fg2)]">{t('Not set')}</span>
        )}
      </div>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-[var(--fg2)]">{label}</span>
      {children}
    </label>
  );
}
