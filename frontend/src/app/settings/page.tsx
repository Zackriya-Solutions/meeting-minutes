'use client';

import React from 'react';
import { useRouter } from 'next/navigation';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { EmbeddingModelSettings } from '@/components/EmbeddingModelSettings';
import { Icon } from '@/components/memento/Icon';
import { useLanguage } from '@/lib/i18n';

export default function SettingsPage() {
  const router = useRouter();
  const { t } = useLanguage();

  return (
    <div className="settings-screen min-h-full">
      <div className="settings-screen__inner">
        <header className="settings-screen__header">
          <button
            type="button"
            onClick={() => router.push('/')}
            className="settings-screen__back no-drag mm-hover"
            aria-label={t('Back')}
          >
            <Icon name="back" />
          </button>
          <h1 className="memento-screen-title">{t('Settings')}</h1>
        </header>

        <main className="settings-screen__content">
          <PreferenceSettings />
          <RecordingSettings />
          <EmbeddingModelSettings />
        </main>
      </div>
    </div>
  );
};
