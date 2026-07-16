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
  {
    value: 'general',
    label: 'General',
    description: 'Notifications, meeting detection, storage, and analytics',
    icon: 'settings',
    keywords: 'уведомления определение встречи хранение аналитика основные',
  },
  {
    value: 'recording',
    label: 'Recordings',
    description: 'Audio recording, files, folders, and recording format',
    icon: 'mic',
    keywords: 'запись аудио файл папка формат mp4',
  },
  {
    value: 'Transcriptionmodels',
    label: 'Transcription',
    description: 'Speech recognition, SaluteSpeech, GigaAM, and speakers',
    icon: 'transcript',
    keywords: 'расшифровка распознавание salutespeech gigaam спикеры',
  },
  {
    value: 'summaryModels',
    label: 'Summary',
    description: 'Summary models, DeepSeek, languages, and templates',
    icon: 'spark',
    keywords: 'суммаризация модель deepseek язык шаблон',
  },
  {
    value: 'providers',
    label: 'Providers',
    description: 'Managed services, gateways, and provider credentials',
    icon: 'library',
    keywords: 'провайдеры шлюз gateway api deepseek salutespeech',
  },
  {
    value: 'privacy',
    label: 'Privacy',
    description: 'Local-only mode, extraction, and knowledge-base chat access',
    icon: 'lock',
    keywords: 'приватность конфиденциальность локальный режим чат база знаний rag',
  },
  {
    value: 'search',
    label: 'Search',
    description: 'Meeting index, semantic search, embeddings, and FRIDA',
    icon: 'search',
    keywords: 'поиск индекс база знаний rag embeddings эмбеддинги frida',
  },
  {
    value: 'beta',
    label: 'Beta',
    description: 'Experimental and preview features',
    icon: 'plus',
    keywords: 'бета эксперименты экспериментальные функции',
  },
] as const satisfies ReadonlyArray<{
  value: string;
  label: string;
  description: string;
  icon: MementoIconName;
  keywords: string;
}>;

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const { t, lang, setLang } = useLanguage();

  // Animation state for tabs
  const [activeTab, setActiveTab] = useState('general');
  const [settingsSearch, setSettingsSearch] = useState('');

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

  const normalizedSettingsSearch = settingsSearch.trim().toLocaleLowerCase(
    lang === 'ru' ? 'ru-RU' : 'en-US',
  );
  const settingsMatches = normalizedSettingsSearch
    ? TABS.filter((tab) =>
        `${t(tab.label)} ${t(tab.description)} ${tab.label} ${tab.description} ${tab.keywords}`
          .toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US')
          .includes(normalizedSettingsSearch),
      )
    : [];

  return (
    <div className="mm-page min-w-0 overflow-hidden !p-0">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 border-b border-[var(--border-subtle)] bg-[var(--bg-canvas)]">
        <div className="mx-auto w-full max-w-6xl px-4 py-5 sm:px-8 sm:py-6">
          <div className="flex min-w-0 flex-wrap items-center gap-3 sm:gap-4">
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

          <div className="relative mt-4 max-w-2xl">
            <label className="mm-field h-11 min-h-11 w-full">
              <Icon name="search" size={17} className="shrink-0 text-[var(--fg3)]" />
              <input
                data-slot="input-group-control"
                value={settingsSearch}
                onChange={(event) => setSettingsSearch(event.target.value)}
                placeholder={t('Search settings…')}
                className="min-w-0 flex-1 border-0 bg-transparent text-sm outline-none"
              />
              {settingsSearch && (
                <button
                  type="button"
                  onClick={() => setSettingsSearch('')}
                  className="text-[var(--fg3)] hover:text-[var(--fg1)]"
                  aria-label={t('Clear')}
                >
                  <Icon name="close" size={16} />
                </button>
              )}
            </label>

            {normalizedSettingsSearch && (
              <div className="absolute left-0 right-0 top-[calc(100%+8px)] z-30 overflow-hidden rounded-2xl border border-[var(--border-strong)] bg-[var(--bg-elevated)] p-2">
                {settingsMatches.length === 0 ? (
                  <p className="px-3 py-4 text-sm text-[var(--fg3)]">{t('No settings found')}</p>
                ) : (
                  settingsMatches.map((tab) => (
                    <button
                      key={tab.value}
                      type="button"
                      onClick={() => {
                        setActiveTab(tab.value);
                        setSettingsSearch('');
                      }}
                      className="flex w-full items-start gap-3 rounded-xl px-3 py-3 text-left hover:bg-[var(--state-hover-bg)]"
                    >
                      <Icon name={tab.icon} size={17} className="mt-0.5 shrink-0 text-[var(--gold)]" />
                      <span className="min-w-0">
                        <span className="block text-sm font-semibold text-[var(--fg1)]">
                          {t(tab.label)}
                        </span>
                        <span className="mt-0.5 block text-xs leading-relaxed text-[var(--fg3)]">
                          {t(tab.description)}
                        </span>
                      </span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
        <div className="mx-auto w-full min-w-0 max-w-6xl p-4 pt-4 sm:p-8 sm:pt-6">
          {/* Tabs */}
          <Tabs value={activeTab} onValueChange={setActiveTab} className="min-w-0">
            <TabsList className="mm-tab-list h-auto w-full max-w-full flex-wrap justify-start overflow-visible">
              {TABS.map((tab) => {
                return (
                  <TabsTrigger
                    key={tab.value}
                    value={tab.value}
                    className="mm-tab shrink-0"
                  >
                    <Icon name={tab.icon} size={16} />
                    {t(tab.label)}
                  </TabsTrigger>
                );
              })}
            </TabsList>

            <TabsContent value="general" className="min-w-0">
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording" className="min-w-0">
              <RecordingSettings />
            </TabsContent>
            <TabsContent value="Transcriptionmodels" className="min-w-0">
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </TabsContent>
            <TabsContent value="summaryModels" className="min-w-0">
              <SummaryModelSettings />
            </TabsContent>
            <TabsContent value="providers" className="min-w-0">
              <ProviderSettings />
            </TabsContent>
            <TabsContent value="privacy" className="min-w-0">
              <PrivacySettings />
            </TabsContent>
            <TabsContent value="search" className="min-w-0">
              <EmbeddingModelSettings />
            </TabsContent>
            <TabsContent value="beta" className="mt-6 min-w-0">
              <BetaSettings />
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
};
