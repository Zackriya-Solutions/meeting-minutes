'use client';

import React, { useState, useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { EmbeddingModelSettings } from '@/components/EmbeddingModelSettings';
import { ProviderSettings } from '@/components/ProviderSettings';
import { PrivacySettings } from '@/components/PrivacySettings';
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Icon, MementoIconName } from '@/components/memento/Icon';
import { useLanguage, type Lang } from '@/lib/i18n';

// Tabs configuration (constant). `label` is the English key; it is translated at
// render time via `t()`.
const TABS = [
  { value: 'general', label: 'General', icon: 'settings' },
  { value: 'recording', label: 'Recordings', icon: 'mic' },
  { value: 'Transcriptionmodels', label: 'Transcription', icon: 'transcript' },
  { value: 'summaryModels', label: 'Summary', icon: 'spark' },
  { value: 'providers', label: 'Providers', icon: 'library' },
  { value: 'privacy', label: 'Privacy', icon: 'lock' },
  { value: 'search', label: 'Search', icon: 'search' },
  { value: 'beta', label: 'Beta', icon: 'plus' }
] as const satisfies ReadonlyArray<{ value: string; label: string; icon: MementoIconName }>;

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const { t, lang, setLang } = useLanguage();

  // Animation state for tabs
  const [activeTab, setActiveTab] = useState('general');

  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get('tab');
    if (requested && TABS.some((tab) => tab.value === requested)) {
      setActiveTab(requested);
    }
  }, []);

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          console.log('Loaded saved transcript config:', config);
          setTranscriptModelConfig({
            provider: config.provider || 'localWhisper',
            model: config.model || 'large-v3',
            apiKey: config.apiKey || null
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  return (
    <div className="mm-page !p-0">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 border-b border-[var(--border-subtle)] bg-[var(--bg-canvas)]">
        <div className="mx-auto max-w-6xl px-8 py-6">
          <div className="flex items-center gap-4">
            <button
              onClick={() => router.back()}
              className="mm-icon-button mm-hover"
              aria-label={t('Back')}
            >
              <Icon name="back" />
            </button>
            <h1 className="mm-page-title">{t('Settings')}</h1>

            {/* Interface language toggle */}
            <div
              className="ml-auto flex items-center gap-1 rounded-full border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-1"
              role="group"
              aria-label={t('Interface language')}
            >
              {(['ru', 'en'] as const).map((l: Lang) => (
                <button
                  key={l}
                  onClick={() => setLang(l)}
                  aria-pressed={lang === l}
                  className={`rounded-full px-3 py-1 text-xs font-semibold transition-colors ${
                    lang === l
                      ? 'bg-[var(--gold)] text-[var(--fg-inverse)]'
                      : 'text-[var(--fg2)] hover:text-[var(--fg1)]'
                  }`}
                >
                  {l === 'ru' ? 'Рус' : 'Eng'}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto p-8 pt-6">
          {/* Tabs */}
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <TabsList className="mm-tab-list h-auto">
              {TABS.map((tab) => {
                return (
                  <TabsTrigger
                    key={tab.value}
                    value={tab.value}
                    className="mm-tab"
                  >
                    <Icon name={tab.icon} size={16} />
                    {t(tab.label)}
                  </TabsTrigger>
                );
              })}
            </TabsList>

            <TabsContent value="general">
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording">
              <RecordingSettings />
            </TabsContent>
            <TabsContent value="Transcriptionmodels">
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </TabsContent>
            <TabsContent value="summaryModels">
              <SummaryModelSettings />
            </TabsContent>
            <TabsContent value="providers">
              <ProviderSettings />
            </TabsContent>
            <TabsContent value="privacy">
              <PrivacySettings />
            </TabsContent>
            <TabsContent value="search">
              <EmbeddingModelSettings />
            </TabsContent>
            <TabsContent value="beta" className="mt-6">
              <BetaSettings />
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
};
