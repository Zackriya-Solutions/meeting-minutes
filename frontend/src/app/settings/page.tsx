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
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Icon, MementoIconName } from '@/components/memento/Icon';

// Tabs configuration (constant)
const TABS = [
  { value: 'general', label: 'Основное', icon: 'settings' },
  { value: 'recording', label: 'Запись', icon: 'mic' },
  { value: 'Transcriptionmodels', label: 'Расшифровка', icon: 'transcript' },
  { value: 'summaryModels', label: 'Суть', icon: 'spark' },
  { value: 'providers', label: 'Провайдеры', icon: 'library' },
  { value: 'search', label: 'Поиск', icon: 'search' },
  { value: 'beta', label: 'Эксперименты', icon: 'plus' }
] as const satisfies ReadonlyArray<{ value: string; label: string; icon: MementoIconName }>;

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();

  // Animation state for tabs
  const [activeTab, setActiveTab] = useState('general');

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
              aria-label="Назад"
            >
              <Icon name="back" />
            </button>
            <h1 className="mm-page-title">Настройки</h1>
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
                    {tab.label}
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
