'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Switch } from '@/components/ui/switch';
import { useT } from '@/lib/i18n';

type PrivacyState = {
  localOnly: boolean;
  extractionEnabled: boolean;
  chatEnabled: boolean;
};

const DEFAULTS: PrivacyState = {
  localOnly: false,
  extractionEnabled: true,
  chatEnabled: true,
};

function settingIsEnabled(value: string | undefined, fallback: boolean): boolean {
  if (value == null) return fallback;
  return value === 'true' || value === '1';
}

export function PrivacySettings() {
  const t = useT();
  const [settings, setSettings] = useState<PrivacyState>(DEFAULTS);
  const [loading, setLoading] = useState(true);
  const [savingKey, setSavingKey] = useState<keyof PrivacyState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    invoke<Record<string, string>>('get_app_settings')
      .then((values) => {
        if (!active) return;
        setSettings({
          localOnly: settingIsEnabled(values['privacy.local_only'], DEFAULTS.localOnly),
          extractionEnabled: settingIsEnabled(
            values['privacy.extraction_enabled'],
            DEFAULTS.extractionEnabled,
          ),
          chatEnabled: settingIsEnabled(values['privacy.chat_enabled'], DEFAULTS.chatEnabled),
        });
      })
      .catch((reason) => {
        if (active) setError(typeof reason === 'string' ? reason : t('Failed to load privacy settings.'));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [t]);

  const update = useCallback(
    async (field: keyof PrivacyState, key: string, enabled: boolean) => {
      const previous = settings[field];
      setError(null);
      setSavingKey(field);
      setSettings((current) => ({ ...current, [field]: enabled }));
      try {
        await invoke('set_app_setting', { key, value: enabled ? 'true' : 'false' });
      } catch (reason) {
        setSettings((current) => ({ ...current, [field]: previous }));
        setError(typeof reason === 'string' ? reason : t('Failed to save privacy settings.'));
      } finally {
        setSavingKey(null);
      }
    },
    [settings, t],
  );

  if (loading) {
    return <div className="mt-6 text-sm text-muted-foreground">{t('Loading privacy settings…')}</div>;
  }

  return (
    <div className="mt-6 max-w-2xl space-y-5">
      <div className="rounded-lg border border-border bg-background p-6">
        <SettingRow
          title={t('Keep meeting content on this device')}
          description={t(
            'Blocks cloud transcription, cloud speaker detection, remote summaries, extraction, and chat before credentials or network clients are used. Local transcription and local AI remain available.',
          )}
          checked={settings.localOnly}
          disabled={savingKey !== null}
          onCheckedChange={(enabled) => update('localOnly', 'privacy.local_only', enabled)}
        />
        {settings.localOnly && (
          <p className="mt-4 rounded-md bg-primary/10 p-3 text-xs text-primary">
            {t('Local-only mode is active. Choose an on-device transcription engine and a local summary model.')}
          </p>
        )}
      </div>

      <div className="rounded-lg border border-border bg-background p-6">
        <h3 className="mb-1 text-lg font-semibold text-foreground">{t('Remote AI permissions')}</h3>
        <p className="mb-5 text-sm text-muted-foreground">
          {t('These permissions apply only when local-only mode is off. Summary generation remains controlled by the selected local or cloud provider.')}
        </p>
        <div className="divide-y divide-border">
          <SettingRow
            title={t('Entity and action extraction')}
            description={t('Allow meeting text to be sent to the configured provider to identify people, topics, and action items.')}
            checked={settings.extractionEnabled}
            disabled={settings.localOnly || savingKey !== null}
            onCheckedChange={(enabled) =>
              update('extractionEnabled', 'privacy.extraction_enabled', enabled)
            }
          />
          <SettingRow
            title={t('Chat and archive questions')}
            description={t('Allow retrieved meeting fragments to be sent to the configured provider when answering questions.')}
            checked={settings.chatEnabled}
            disabled={settings.localOnly || savingKey !== null}
            onCheckedChange={(enabled) => update('chatEnabled', 'privacy.chat_enabled', enabled)}
            className="pt-5"
          />
        </div>
      </div>

      <p className="text-xs text-muted-foreground">
        {t('Provider credentials are stored locally and are never returned to the app interface after saving.')}
      </p>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

function SettingRow({
  title,
  description,
  checked,
  disabled,
  onCheckedChange,
  className = '',
}: {
  title: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onCheckedChange: (enabled: boolean) => void;
  className?: string;
}) {
  return (
    <div className={`flex items-start justify-between gap-6 ${className}`}>
      <div>
        <h4 className="font-medium text-foreground">{title}</h4>
        <p className="mt-1 text-sm leading-5 text-muted-foreground">{description}</p>
      </div>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
        aria-label={title}
      />
    </div>
  );
}
