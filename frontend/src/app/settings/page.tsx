'use client';

import React, { useState, useEffect, useLayoutEffect, useRef } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon, SparkleIcon, FlaskConical } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ThemeToggle } from '@/components/ThemeToggle';

// Tabs configuration (constant)
const TABS = [
  { value: 'general', label: 'General', icon: Settings2 },
  { value: 'recording', label: 'Recordings', icon: Mic },
  { value: 'Transcriptionmodels', label: 'Transcription', icon: DatabaseIcon },
  { value: 'summaryModels', label: 'Summary', icon: SparkleIcon },
  { value: 'beta', label: 'Beta', icon: FlaskConical }
] as const;

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();

  // Animation state for tabs
  const [activeTab, setActiveTab] = useState('general');
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [underlineStyle, setUnderlineStyle] = useState({ left: 0, width: 0 });

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          console.log('Loaded saved transcript config:', config);
          setTranscriptModelConfig({
            provider: config.provider || 'local',
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

  // Update underline position when active tab changes
  useLayoutEffect(() => {
    const activeIndex = TABS.findIndex(tab => tab.value === activeTab);
    const activeTabElement = tabRefs.current[activeIndex];

    if (activeTabElement) {
      const { offsetLeft, offsetWidth } = activeTabElement;
      setUnderlineStyle({ left: offsetLeft, width: offsetWidth });
    }
  }, [activeTab]);

  return (
    <div className="flex h-screen flex-col bg-canvas">
      <header className="sticky top-0 z-sticky border-b border-line bg-canvas">
        <div className="mx-auto flex h-14 max-w-5xl items-center gap-3 px-8">
          <button
            onClick={() => router.back()}
            className="flex h-8 w-8 items-center justify-center rounded-md text-ink-muted transition-colors duration-fast hover:bg-ink/5 hover:text-ink"
            aria-label="Back"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
          <h1 className="text-xl font-semibold text-ink">Settings</h1>
        </div>
      </header>

      <div className="scrollbar-slim flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl px-8 pb-16">
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            {/* The strip's shape lives in the `underline` Tabs variant, which
                deliberately draws no indicator — the spring below owns it. */}
            <TabsList variant="underline">
              {TABS.map((tab, index) => {
                const Icon = tab.icon;
                return (
                  <TabsTrigger
                    key={tab.value}
                    value={tab.value}
                    ref={el => { tabRefs.current[index] = el }}
                  >
                    <Icon className="h-4 w-4" aria-hidden />
                    {tab.label}
                  </TabsTrigger>
                );
              })}

              {/* The one place a spring is right: it tracks a direct manipulation. */}
              <motion.div
                aria-hidden
                className="absolute -bottom-px z-20 h-0.5 bg-brand"
                layoutId="underline"
                style={{ left: underlineStyle.left, width: underlineStyle.width }}
                transition={{ type: 'spring', stiffness: 500, damping: 42 }}
              />
            </TabsList>

            <TabsContent value="general" className="mt-6">
              {/* Theme lives here rather than in a hidden menu — it is the one
                  setting a user changes because the room changed. */}
              <section className="mb-8 flex flex-wrap items-center justify-between gap-3 border-b border-line pb-6">
                <div>
                  <h2 className="text-base font-medium text-ink">Appearance</h2>
                  <p className="mt-0.5 text-sm text-ink-muted">
                    System follows your OS setting.
                  </p>
                </div>
                <ThemeToggle />
              </section>
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording" className="mt-6">
              <RecordingSettings />
            </TabsContent>
            <TabsContent value="Transcriptionmodels" className="mt-6">
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </TabsContent>
            <TabsContent value="summaryModels" className="mt-6">
              <SummaryModelSettings />
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
