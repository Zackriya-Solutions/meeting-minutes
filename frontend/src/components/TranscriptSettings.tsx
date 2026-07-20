import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '@/lib/i18n';
import { GigaamModelManager } from './GigaamModelManager';
import { Label } from './ui/label';
import { Switch } from './ui/switch';

export interface TranscriptModelProps {
  // Kept broad for backward compatibility with persisted configs. The current UI
  // offers GigaAM only; anything else is migrated to it on Settings mount.
  provider: 'localWhisper' | 'parakeet' | 'gigaam' | 'salutespeech' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
  model: string;
  apiKey?: string | null;
}

export interface TranscriptSettingsProps {
  transcriptModelConfig: TranscriptModelProps;
  setTranscriptModelConfig: (config: TranscriptModelProps) => void;
  onModelSelect?: () => void;
}

// Fallback when the variant selection can't be read; matches GigaamVariant::default().
const GIGAAM_FALLBACK_MODEL = 'gigaam-v3-e2e-rnnt-fp32';
// GigaAM is the only supported engine (2026-07-20): on a real 31-min meeting the
// SaluteSpeech cloud matched 80.4% of reference words vs GigaAM's 92.4%, and cloud
// diarization found 4 of 7 speakers vs local's 7/7. Cloud settings were removed.
const ALLOWED = new Set(['gigaam']);

/// The persisted model string for the GigaAM provider reflects the variant the engine
/// will actually run (Settings → Model variant), so logs and metadata stop claiming
/// "e2e-ctc" regardless of what's loaded.
async function gigaamModelName(): Promise<string> {
  try {
    const status = await invoke<{ selected?: string }>('gigaam_status');
    return status?.selected ? `gigaam-v3-${status.selected}` : GIGAAM_FALLBACK_MODEL;
  } catch {
    return GIGAAM_FALLBACK_MODEL;
  }
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig }: TranscriptSettingsProps) {
  const t = useT();
  const provider = transcriptModelConfig.provider;

  // Migrate any persisted non-local provider (older builds offered SaluteSpeech) to
  // GigaAM with the variant-accurate model label.
  useEffect(() => {
    if (!ALLOWED.has(provider)) {
      void (async () => {
        const model = await gigaamModelName();
        const config: TranscriptModelProps = { provider: 'gigaam', model, apiKey: null };
        setTranscriptModelConfig(config);
        invoke('api_save_transcript_config', { provider: 'gigaam', model, apiKey: null })
          .catch((error) => console.error('Failed to save transcript config:', error));
      })();
    }
  }, [provider, setTranscriptModelConfig]);

  return (
    <div className="space-y-4 pb-6">
      <div>
        <Label className="mb-1 block text-sm font-medium text-[var(--fg2)]">{t('Transcription engine')}</Label>
        <p className="text-sm text-[var(--fg2)]">{t('Meetings are transcribed on this device.')}</p>
      </div>

      <div className="grid gap-2">
        <EngineOption
          active
          onClick={() => {}}
          title={t('GigaAM v3 · on-device')}
          subtitle={t('Sber · offline Russian speech recognition with punctuation. Private — audio never leaves your machine.')}
        />
      </div>

      <GigaamModelManager />

      <MicSensitivitySetting />

      <RefinementSetting />
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


// Speech-detection sensitivity. Bluetooth/HFP mics (AirPods used as input) produce weak,
// narrowband audio that the VAD skips — this switches the backend to a more sensitive
// profile (persisted to app_settings_kv `vad.high_sensitivity`, applied next recording).
function MicSensitivitySetting() {
  const t = useT();
  const [enabled, setEnabled] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const settings = await invoke<Record<string, string>>('get_app_settings');
        setEnabled((settings?.['vad.high_sensitivity'] || '').trim().toLowerCase() === 'true');
      } catch {
        // default off
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  const toggle = (next: boolean) => {
    setEnabled(next);
    invoke('set_app_setting', { key: 'vad.high_sensitivity', value: next ? 'true' : 'false' })
      .catch((error) => console.error('Failed to save VAD sensitivity setting:', error));
  };

  if (!loaded) return null;

  return (
    <div className="flex items-start justify-between gap-4 border-t border-[var(--border-subtle)] pt-4">
      <div>
        <Label className="block text-sm font-medium text-[var(--fg2)]">{t('Boost sensitivity for Bluetooth / quiet microphones')}</Label>
        <p className="mt-0.5 text-xs text-[var(--fg3)]">{t('Detects speech more aggressively for low-quality inputs like AirPods used as a mic (Bluetooth hands-free mode). Turn on if phrases are being skipped. Applies to the next recording.')}</p>
      </div>
      <Switch checked={enabled} onCheckedChange={toggle} />
    </div>
  );
}

// Post-meeting refinement: after a recording is saved, the meeting is automatically
// re-transcribed through the batch path (longer VAD context → complete sentences and
// correct numbers), diarized, and exported with speaker labels. Enabled by default;
// persisted to app_settings_kv `refinement.auto` ('false' disables).
function RefinementSetting() {
  const t = useT();
  const [enabled, setEnabled] = useState(true);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const settings = await invoke<Record<string, string>>('get_app_settings');
        setEnabled((settings?.['refinement.auto'] || '').trim().toLowerCase() !== 'false');
      } catch {
        // default on
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  const toggle = (next: boolean) => {
    setEnabled(next);
    invoke('set_app_setting', { key: 'refinement.auto', value: next ? 'true' : 'false' })
      .catch((error) => console.error('Failed to save refinement setting:', error));
  };

  if (!loaded) return null;

  return (
    <div className="flex items-start justify-between gap-4 border-t border-[var(--border-subtle)] pt-4">
      <div>
        <Label className="block text-sm font-medium text-[var(--fg2)]">{t('Polish transcript after the meeting')}</Label>
        <p className="mt-0.5 text-xs text-[var(--fg3)]">{t('When a recording ends, the meeting is re-transcribed from the saved audio with full context and split by speaker. The live transcript is replaced once the pass finishes (a few minutes).')}</p>
      </div>
      <Switch checked={enabled} onCheckedChange={toggle} />
    </div>
  );
}
