'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { HardDrive, MessageSquare, Sparkles, type LucideIcon } from '@/components/deslop-icons';
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
    <div className="space-y-4">
      <SettingRow
        icon={HardDrive}
        title={t('Keep meeting content on this device')}
        description={t(
          'Blocks cloud transcription, cloud speaker detection, remote summaries, extraction, and chat before credentials or network clients are used. Local transcription and local AI remain available.',
        )}
        checked={settings.localOnly}
        disabled={savingKey !== null}
        onCheckedChange={(enabled) => update('localOnly', 'privacy.local_only', enabled)}
      />

      <SettingRow
        icon={Sparkles}
        title={t('Entity and action extraction')}
        description={t('Allow meeting text to be sent to the configured provider to identify people, topics, and action items.')}
        checked={settings.extractionEnabled}
        disabled={settings.localOnly || savingKey !== null}
        onCheckedChange={(enabled) =>
          update('extractionEnabled', 'privacy.extraction_enabled', enabled)
        }
      />

      <SettingRow
        icon={MessageSquare}
        title={t('Chat and archive questions')}
        description={t('Allow retrieved meeting fragments to be sent to the configured provider when answering questions.')}
        checked={settings.chatEnabled}
        disabled={settings.localOnly || savingKey !== null}
        onCheckedChange={(enabled) => update('chatEnabled', 'privacy.chat_enabled', enabled)}
      />

      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

function SettingRow({
  icon: Icon,
  title,
  description,
  checked,
  disabled,
  onCheckedChange,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onCheckedChange: (enabled: boolean) => void;
}) {
  return (
    <section className="settings-section settings-cell">
      <div className="settings-cell__row">
        <span className="settings-cell__avatar" aria-hidden="true">
          <Icon size={20} />
        </span>
        <div className="settings-cell__text">
          <h3 className="settings-cell__label">{title}</h3>
          <p className="settings-cell__caption">{description}</p>
        </div>
        <Switch
          className="shrink-0"
          checked={checked}
          disabled={disabled}
          onCheckedChange={onCheckedChange}
          aria-label={title}
        />
      </div>
    </section>
  );
}
