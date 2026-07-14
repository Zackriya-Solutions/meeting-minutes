import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AlertTriangle, CheckCircle2, KeyRound, Loader2 } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';
import { GigaamModelManager } from './GigaamModelManager';
import { Label } from './ui/label';

export interface TranscriptModelProps {
  // Kept broad for backward compatibility. The current UI offers GigaAM and SaluteSpeech.
  provider: 'localWhisper' | 'parakeet' | 'gigaam' | 'salutespeech' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
  model: string;
  apiKey?: string | null;
}

export interface TranscriptSettingsProps {
  transcriptModelConfig: TranscriptModelProps;
  setTranscriptModelConfig: (config: TranscriptModelProps) => void;
  onModelSelect?: () => void;
}

const GIGAAM_MODEL = 'gigaam-v3-e2e-ctc';
const SALUTE_MODEL = 'salutespeech-stream-v2';
const ALLOWED = new Set(['gigaam', 'salutespeech']);

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig }: TranscriptSettingsProps) {
  const t = useT();
  const provider = transcriptModelConfig.provider;

  useEffect(() => {
    if (!ALLOWED.has(provider)) {
      const config: TranscriptModelProps = { provider: 'salutespeech', model: SALUTE_MODEL, apiKey: null };
      setTranscriptModelConfig(config);
      invoke('api_save_transcript_config', { provider: 'salutespeech', model: SALUTE_MODEL, apiKey: null })
        .catch((error) => console.error('Failed to save SaluteSpeech transcript config:', error));
    }
  }, [provider, setTranscriptModelConfig]);

  const selectProvider = useCallback((next: 'gigaam' | 'salutespeech') => {
    const model = next === 'salutespeech' ? SALUTE_MODEL : GIGAAM_MODEL;
    const config: TranscriptModelProps = { provider: next, model, apiKey: null };
    setTranscriptModelConfig(config);
    invoke('api_save_transcript_config', { provider: next, model, apiKey: null })
      .catch((error) => console.error('Failed to save transcript config:', error));
  }, [setTranscriptModelConfig]);

  return (
    <div className="space-y-4 pb-6">
      <div>
        <Label className="mb-1 block text-sm font-medium text-[var(--fg2)]">{t('Transcription engine')}</Label>
        <p className="text-sm text-[var(--fg2)]">{t('Choose how meetings are transcribed.')}</p>
      </div>

      <div className="grid gap-2">
        <EngineOption
          active={provider === 'gigaam'}
          onClick={() => selectProvider('gigaam')}
          title={t('GigaAM v3 · on-device')}
          subtitle={t('Sber · offline Russian speech recognition with punctuation. Private — audio never leaves your machine.')}
        />
        <EngineOption
          active={provider === 'salutespeech'}
          onClick={() => selectProvider('salutespeech')}
          title={t('SaluteSpeech · Sber cloud')}
          subtitle={t('Cloud recognition via speech.giga.chat (ru-RU). Audio is sent to Sber for transcription; needs an internet connection.')}
        />
      </div>

      {provider === 'gigaam' && <GigaamModelManager />}
      {provider === 'salutespeech' && <SaluteSpeechSettings />}

      <DiarizationEngineSetting />
    </div>
  );
}

/**
 * Speaker-detection (diarization) engine — independent of the transcription engine; runs
 * post-meeting via the "Speakers" button. Local uses on-device ONNX models; SaluteSpeech
 * uses Sber's cloud speaker separation (reuses the SaluteSpeech Authorization Key above).
 * Persisted to app_settings_kv `diarization.provider`.
 */
function DiarizationEngineSetting() {
  const t = useT();
  const [provider, setProvider] = useState<'local' | 'salutespeech'>('local');
  const [saluteKeySet, setSaluteKeySet] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const settings = await invoke<Record<string, string>>('get_app_settings');
        const p = (settings?.['diarization.provider'] || 'local').trim();
        setProvider(p === 'salutespeech' ? 'salutespeech' : 'local');
        setSaluteKeySet(!!settings?.['salutespeech.auth_key'] && settings['salutespeech.auth_key'].length > 0);
      } catch {
        // default to local
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  const choose = (p: 'local' | 'salutespeech') => {
    setProvider(p);
    invoke('set_app_setting', { key: 'diarization.provider', value: p })
      .catch((error) => console.error('Failed to save diarization provider:', error));
  };

  if (!loaded) return null;

  return (
    <div className="space-y-2 border-t border-[var(--border-subtle)] pt-4">
      <Label className="block text-sm font-medium text-[var(--fg2)]">{t('Speaker detection')}</Label>
      <p className="text-sm text-[var(--fg2)]">{t('Who said each line — runs after a meeting via the “Speakers” button.')}</p>
      <div className="grid gap-2">
        <EngineOption
          active={provider === 'local'}
          onClick={() => choose('local')}
          title={t('Local · on-device')}
          subtitle={t('pyannote-style models (~35 MB, one-time download). Private — audio stays on your machine.')}
        />
        <EngineOption
          active={provider === 'salutespeech'}
          onClick={() => choose('salutespeech')}
          title={t('SaluteSpeech · Sber cloud')}
          subtitle={saluteKeySet
            ? t('Cloud speaker separation using your SaluteSpeech key. Audio is sent to Sber.')
            : t('Needs a SaluteSpeech Authorization Key (set it above). Audio is sent to Sber.')}
        />
      </div>
    </div>
  );
}

function EngineOption({ active, onClick, title, subtitle }: {
  active: boolean;
  onClick: () => void;
  title: string;
  subtitle: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`mm-press flex items-start gap-3 rounded-[var(--radius-16)] border p-4 text-left transition-colors ${
        active
          ? 'border-[var(--gold-border)] bg-[var(--gold-soft)]'
          : 'border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:border-[var(--border-strong)] hover:bg-[var(--state-hover-bg)]'
      }`}
    >
      <span className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${active ? 'border-[var(--gold)]' : 'border-[var(--border-strong)]'}`}>
        {active && <span className="h-2 w-2 rounded-full bg-[var(--gold)]" />}
      </span>
      <span>
        <span className="block text-sm font-medium text-[var(--fg1)]">{title}</span>
        <span className="mt-0.5 block text-xs text-[var(--fg2)]">{subtitle}</span>
      </span>
    </button>
  );
}

function SaluteSpeechSettings() {
  const t = useT();
  return (
    <div className="rounded-[var(--radius-24)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-5">
      <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('SaluteSpeech · managed')}</h3>
      <p className="mt-1 text-xs text-[var(--fg3)]">{t('Ready to use through the Memento gateway. No Authorization Key is required.')}</p>
    </div>
  );
  /* Legacy BYOK controls intentionally retained below for easy future opt-in. */
  /* eslint-disable no-unreachable */
  const [configured, setConfigured] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [authKey, setAuthKey] = useState('');
  const [model, setModel] = useState('universal_turbo');
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const settings = await invoke<Record<string, string>>('get_app_settings');
      setConfigured(!!settings?.['salutespeech.auth_key']);
      if (settings?.['salutespeech.model']) setModel(settings['salutespeech.model']);
    } catch {
      // Treat unreadable settings as not configured.
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const save = useCallback(async () => {
    setError(null);
    setSaved(false);
    const updates: [string, string][] = [];
    if (authKey.trim()) updates.push(['salutespeech.auth_key', authKey.trim()]);
    if (model.trim()) updates.push(['salutespeech.model', model.trim()]);
    if (updates.length === 0) {
      setError(t('Enter your Authorization Key first.'));
      return;
    }
    setSaving(true);
    try {
      for (const [key, value] of updates) await invoke('set_app_setting', { key, value });
      setAuthKey('');
      await refresh();
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2500);
    } catch (caught) {
      setError(typeof caught === 'string' ? caught : t('Failed to save credentials.'));
    } finally {
      setSaving(false);
    }
  }, [authKey, model, refresh, t]);

  if (!loaded) {
    return <div className="flex items-center gap-2 text-sm text-[var(--fg3)]"><Loader2 className="h-4 w-4 animate-spin" /> {t('Loading…')}</div>;
  }

  return (
    <div className="rounded-[var(--radius-24)] border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-5">
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('SaluteSpeech credentials')}</h3>
          <p className="text-xs text-[var(--fg3)]">Sber SmartSpeech · streaming recognition v2</p>
        </div>
        {configured ? (
          <span className="flex items-center gap-1 rounded-full bg-[color-mix(in_srgb,var(--success)_12%,transparent)] px-2 py-0.5 text-xs font-medium text-[var(--success)]"><CheckCircle2 className="h-3.5 w-3.5" /> {t('Configured')}</span>
        ) : (
          <span className="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-xs text-[var(--fg2)]">{t('Not set')}</span>
        )}
      </div>

      <label className="block">
        <span className="mb-1 block text-xs font-medium text-[var(--fg2)]">{t('Authorization key')}</span>
        <input
          type="password"
          value={authKey}
          onChange={(event) => setAuthKey(event.target.value)}
          placeholder={configured ? t('•••••••• (saved — enter to replace)') : 'base64(client_id:client_secret)'}
          className="w-full rounded-[var(--radius-12)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg1)] focus:border-[var(--gold)] focus:outline-none focus:ring-2 focus:ring-[var(--gold-ring)]"
          autoComplete="off"
        />
        <p className="mt-1 text-xs text-[var(--fg3)]">{t('The Sber “Authorization Key” from your account — base64 of login:password (or ClientID:ClientSecret).')}</p>
      </label>

      <label className="mt-3 block">
        <span className="mb-1 block text-xs font-medium text-[var(--fg2)]">{t('Recognition model')}</span>
        <select value={model} onChange={(event) => setModel(event.target.value)} className="w-full rounded-[var(--radius-12)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--fg1)] focus:border-[var(--gold)] focus:outline-none">
          <option value="universal_turbo">universal_turbo ({t('default')}, {t('fast')})</option>
          <option value="transcribation_hq">transcribation_hq ({t('high quality')})</option>
        </select>
      </label>

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <button type="button" onClick={save} disabled={saving} className="mm-press flex items-center gap-2 rounded-full bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)] disabled:cursor-not-allowed disabled:opacity-50">
          {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <KeyRound className="h-4 w-4" />}
          {t('Save key')}
        </button>
        {saved && <span className="flex items-center gap-1.5 text-sm text-[var(--success)]"><CheckCircle2 className="h-4 w-4" /> {t('Saved')}</span>}
        {error && <span className="flex items-center gap-1.5 text-sm text-[var(--danger)]"><AlertTriangle className="h-4 w-4" /> {error}</span>}
      </div>

      <p className="mt-3 text-xs text-[var(--fg3)]">{t('Recognition runs in the cloud (ru-RU) via speech.giga.chat. Requires an internet connection; the on-device GigaAM engine remains available as an offline fallback. Speaker labels come from local diarization (the cloud recognizer returns text only).')}</p>
    </div>
  );
}
