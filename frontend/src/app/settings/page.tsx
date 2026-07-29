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
    <div className="mm-page min-w-0 overflow-hidden !p-0">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 bg-background">
        <div className="mx-auto w-full max-w-6xl px-4 py-5 sm:px-8 sm:py-6">
          <div className="flex min-w-0 flex-wrap items-center gap-3 sm:gap-4">
            <button
              onClick={() => router.push('/')}
              className="mm-icon-button mm-hover"
              aria-label={t('Back')}
            >
              <Icon name="back" />
            </button>
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
        <div className="mx-auto w-full min-w-0 max-w-6xl p-4 pt-4 sm:p-8 sm:pt-6">
          <div className="min-w-0 space-y-6">
            <PreferenceSettings />
            <RecordingSettings />
            <EmbeddingModelSettings />
          </div>
        </div>
      </div>
    </div>
  );
};
