import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, Loader2, AlertTriangle, KeyRound } from 'lucide-react';
import { Label } from './ui/label';
import { GigaamModelManager } from './GigaamModelManager';

export interface TranscriptModelProps {
    // Union kept broad for backward compatibility with stored configs. The UI offers
    // GigaAM (on-device) and SaluteSpeech (Sber cloud); other values are migrated to GigaAM.
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
// Providers the UI can actually select; anything else (legacy Whisper/Parakeet/…) is
// migrated to GigaAM on mount.
const ALLOWED = new Set(['gigaam', 'salutespeech']);

/**
 * Transcription settings. Two engines: GigaAM v3 (on-device, private) and SaluteSpeech
 * (Sber cloud streaming, with built-in speaker separation). Legacy providers are
 * migrated to GigaAM on mount.
 */
export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig }: TranscriptSettingsProps) {
    const provider = transcriptModelConfig.provider;

    useEffect(() => {
        if (!ALLOWED.has(provider)) {
            const cfg: TranscriptModelProps = { provider: 'gigaam', model: GIGAAM_MODEL, apiKey: null };
            setTranscriptModelConfig(cfg);
            invoke('api_save_transcript_config', { provider: 'gigaam', model: GIGAAM_MODEL, apiKey: null })
                .catch((e) => console.error('Failed to save GigaAM transcript config:', e));
        }
    }, [provider, setTranscriptModelConfig]);

    const selectProvider = useCallback(
        (next: 'gigaam' | 'salutespeech') => {
            const model = next === 'salutespeech' ? SALUTE_MODEL : GIGAAM_MODEL;
            const cfg: TranscriptModelProps = { provider: next, model, apiKey: null };
            setTranscriptModelConfig(cfg);
            invoke('api_save_transcript_config', { provider: next, model, apiKey: null }).catch((e) =>
                console.error('Failed to save transcript config:', e),
            );
        },
        [setTranscriptModelConfig],
    );

    return (
        <div className="space-y-4 pb-6">
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">Transcription engine</Label>
                <p className="text-sm text-gray-500">Choose how meetings are transcribed.</p>
            </div>

            <div className="grid gap-2">
                <EngineOption
                    active={provider === 'gigaam'}
                    onClick={() => selectProvider('gigaam')}
                    title="GigaAM v3 · on-device"
                    subtitle="Sber · offline Russian speech recognition with punctuation. Private — audio never leaves your machine."
                />
                <EngineOption
                    active={provider === 'salutespeech'}
                    onClick={() => selectProvider('salutespeech')}
                    title="SaluteSpeech · Sber cloud"
                    subtitle="Cloud recognition via speech.giga.chat (ru-RU). Audio is sent to Sber for transcription; needs an internet connection."
                />
            </div>

            {provider === 'gigaam' && <GigaamModelManager />}
            {provider === 'salutespeech' && <SaluteSpeechSettings />}
        </div>
    );
}

function EngineOption({
    active,
    onClick,
    title,
    subtitle,
}: {
    active: boolean;
    onClick: () => void;
    title: string;
    subtitle: string;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            className={`flex items-start gap-3 rounded-xl border p-4 text-left transition-colors ${
                active ? 'border-blue-400 bg-blue-50/50 ring-1 ring-blue-300' : 'border-gray-200 hover:border-gray-300'
            }`}
        >
            <span
                className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${
                    active ? 'border-blue-500' : 'border-gray-300'
                }`}
            >
                {active && <span className="h-2 w-2 rounded-full bg-blue-500" />}
            </span>
            <span>
                <span className="block text-sm font-medium text-gray-900">{title}</span>
                <span className="mt-0.5 block text-xs text-gray-500">{subtitle}</span>
            </span>
        </button>
    );
}

/**
 * SaluteSpeech credentials. The Sber "Authorization Key" is stored write-only in
 * `app_settings_kv` (`salutespeech.auth_key`) via `set_app_setting`, mirroring the
 * GigaChat provider settings.
 */
function SaluteSpeechSettings() {
    const [configured, setConfigured] = useState(false);
    const [loaded, setLoaded] = useState(false);
    const [authKey, setAuthKey] = useState('');
    const [model, setModel] = useState('voice_messaging');
    const [saving, setSaving] = useState(false);
    const [saved, setSaved] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        try {
            const s = await invoke<Record<string, string>>('get_app_settings');
            setConfigured(!!s?.['salutespeech.auth_key'] && s['salutespeech.auth_key'].length > 0);
            if (s?.['salutespeech.model']) setModel(s['salutespeech.model']);
        } catch {
            /* settings unreadable → treat as not configured */
        } finally {
            setLoaded(true);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    const save = useCallback(async () => {
        setError(null);
        setSaved(false);
        const updates: [string, string][] = [];
        if (authKey.trim()) updates.push(['salutespeech.auth_key', authKey.trim()]);
        if (model.trim()) updates.push(['salutespeech.model', model.trim()]);
        if (updates.length === 0) {
            setError('Enter your Authorization Key first.');
            return;
        }
        setSaving(true);
        try {
            for (const [key, value] of updates) {
                await invoke('set_app_setting', { key, value });
            }
            setAuthKey('');
            await refresh();
            setSaved(true);
            setTimeout(() => setSaved(false), 2500);
        } catch (e) {
            setError(typeof e === 'string' ? e : 'Failed to save credentials.');
        } finally {
            setSaving(false);
        }
    }, [authKey, model, refresh]);

    if (!loaded) {
        return (
            <div className="flex items-center gap-2 text-sm text-gray-400">
                <Loader2 className="h-4 w-4 animate-spin" /> Loading…
            </div>
        );
    }

    return (
        <div className="rounded-xl border border-gray-200 p-5">
            <div className="mb-4 flex items-center justify-between">
                <div>
                    <h3 className="text-sm font-semibold text-gray-900">SaluteSpeech credentials</h3>
                    <p className="text-xs text-gray-400">Sber SmartSpeech · streaming recognition v2</p>
                </div>
                {configured ? (
                    <span className="flex items-center gap-1 rounded-full bg-green-50 px-2 py-0.5 text-xs font-medium text-green-700">
                        <CheckCircle2 className="h-3.5 w-3.5" /> Configured
                    </span>
                ) : (
                    <span className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-500">Not set</span>
                )}
            </div>

            <label className="block">
                <span className="mb-1 block text-xs font-medium text-gray-600">Authorization key</span>
                <input
                    type="password"
                    value={authKey}
                    onChange={(e) => setAuthKey(e.target.value)}
                    placeholder={configured ? '•••••••• (saved — enter to replace)' : 'base64(client_id:client_secret)'}
                    className="w-full rounded-lg border border-gray-200 px-3 py-2 text-sm text-gray-800 focus:border-blue-400 focus:outline-none"
                    autoComplete="off"
                />
                <p className="mt-1 text-xs text-gray-400">
                    The Sber “Authorization Key” from your account — base64 of login:password (or ClientID:ClientSecret).
                </p>
            </label>

            <label className="mt-3 block">
                <span className="mb-1 block text-xs font-medium text-gray-600">Recognition model</span>
                <select
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    className="w-full rounded-lg border border-gray-200 px-3 py-2 text-sm text-gray-800 focus:border-blue-400 focus:outline-none"
                >
                    <option value="voice_messaging">voice_messaging (default)</option>
                    <option value="transcribation_hq">transcribation_hq (high quality)</option>
                    <option value="universal_turbo">universal_turbo (fast)</option>
                </select>
            </label>

            <div className="mt-4 flex items-center gap-3">
                <button
                    type="button"
                    onClick={save}
                    disabled={saving}
                    className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300"
                >
                    {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <KeyRound className="h-4 w-4" />}
                    Save key
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

            <p className="mt-3 text-xs text-gray-400">
                Recognition runs in the cloud (ru-RU) via speech.giga.chat. Requires an internet connection; the
                on-device GigaAM engine remains available as an offline fallback. Speaker labels come from local
                diarization (the cloud recognizer returns text only).
            </p>
        </div>
    );
}
