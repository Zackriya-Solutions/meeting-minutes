'use client';

import React, { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryLanguageSettings } from '@/components/SummaryLanguageSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { EmbeddingModelSettings } from '@/components/EmbeddingModelSettings';
import { PrivacySettings } from '@/components/PrivacySettings';
import { CalendarSettings } from '@/components/CalendarSettings';
import { useConfig } from '@/contexts/ConfigContext';
import { useLanguage } from '@/lib/i18n';

export default function SettingsPage() {
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const { t } = useLanguage();

  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get('tab');
    if (!requested) return;

    window.requestAnimationFrame(() => {
      document.getElementById(`settings-${requested}`)?.scrollIntoView({ block: 'start' });
    });
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
    <div className="settings-screen min-h-full">
      <div className="settings-screen__inner">
        <header className="settings-screen__header">
          <h1 className="memento-screen-title">{t('Settings')}</h1>
        </header>

        <main className="settings-screen__content">
          <section id="settings-general" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('General')}</h2>
            <div className="settings-screen__section-content">
              <PreferenceSettings />
            </div>
          </section>

          <section id="settings-recording" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Recordings')}</h2>
            <div className="settings-screen__section-content">
              <RecordingSettings />
            </div>
          </section>

          <section id="settings-Transcriptionmodels" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Transcription')}</h2>
            <div className="settings-screen__section-content">
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </div>
          </section>

          <section id="settings-summaryModels" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Summary')}</h2>
            <div className="settings-screen__section-content">
              <SummaryLanguageSettings />
            </div>
          </section>

          <section id="settings-privacy" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Privacy')}</h2>
            <div className="settings-screen__section-content">
              <PrivacySettings />
            </div>
          </section>

          <section id="settings-calendar" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Calendar')}</h2>
            <div className="settings-screen__section-content">
              <CalendarSettings />
            </div>
          </section>

          <section id="settings-search" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Search')}</h2>
            <div className="settings-screen__section-content">
              <EmbeddingModelSettings />
            </div>
          </section>

          <section id="settings-beta" className="settings-screen__group">
            <h2 className="settings-screen__section-title">{t('Beta')}</h2>
            <div className="settings-screen__section-content">
              <BetaSettings />
            </div>
          </section>
        </main>
      </div>
    </div>
  );
};
