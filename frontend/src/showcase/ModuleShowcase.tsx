'use client';

import React, { Component, type ComponentType, type ReactNode } from 'react';
import { OnboardingContext, type OnboardingContextType } from '@/contexts/OnboardingContext';
import { TooltipProvider } from '@/components/ui/tooltip';
import { PrimitiveModuleShowcase } from './PrimitiveModuleShowcase';
import { SidebarContext, type SidebarContextType } from '@/components/Sidebar/SidebarProvider';

type ProductionModule = Record<string, unknown>;

class SpecimenBoundary extends Component<{ name: string; children: ReactNode }, { error?: Error }> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="rounded-lg border border-border bg-muted p-4 text-sm text-muted-foreground">
          Для {this.props.name} нужен продуктовый контекст: {this.state.error.message}
        </div>
      );
    }
    return this.props.children;
  }
}

function isRenderable(value: unknown): value is ComponentType<Record<string, unknown>> {
  return typeof value === 'function' || (
    typeof value === 'object' && value !== null && '$$typeof' in value
  );
}

function defaultProps(name: string): Record<string, unknown> {
  const noop = () => undefined;
  const common = {
    children: 'Пример',
    title: name,
    description: 'Production-компонент Memento',
    label: name,
    value: 'example',
    defaultValue: 'example',
    placeholder: 'Введите текст',
    open: true,
    checked: true,
    disabled: false,
    isActive: true,
    isOpen: true,
    isRecording: false,
    progress: 62,
    percentage: 62,
    confidence: 0.86,
    rmsLevel: 0.42,
    peakLevel: 0.61,
    deviceName: 'MacBook Microphone',
    onClick: noop,
    onChange: noop,
    onOpenChange: noop,
    onClose: noop,
    onDismiss: noop,
    onAction: noop,
    onPrimaryAction: noop,
  };

  if (name === 'BlockComponent') {
    return {
      ...common,
      block: { id: 'showcase-block', type: 'text', content: 'Результат встречи можно отредактировать прямо здесь.' },
      isSelected: false,
      onTypeChange: noop,
      onMouseDown: noop,
      onMouseEnter: noop,
      onMouseUp: noop,
      onKeyDown: noop,
    };
  }

  if (name === 'SummaryMessage') {
    return {
      ...common,
      actualDurationSeconds: 1860,
      speakers: [],
      summaryPanelProps: {
        aiSummary: null,
        summaryStatus: 'idle',
        getSummaryStatusMessage: () => 'Итоги готовы',
        onGenerateSummary: noop,
        onRegenerateSummary: noop,
      },
    };
  }

  if (name === 'TranscriptPanel') {
    return {
      ...common,
      transcripts: [],
      segments: [],
      customPrompt: '',
      onPromptChange: noop,
      onCopyTranscript: noop,
      onOpenMeetingFolder: async () => undefined,
      isRecording: false,
      markedMoments: [],
    };
  }

  if (name === 'VirtualizedTranscriptView') {
    return {
      ...common,
      segments: [
        {
          id: 'showcase-segment-1',
          text: 'Давайте начнём с главного результата встречи.',
          timestamp: 12,
          recognizedAt: '10:24',
          speaker_id: 0,
          reaction: 'angry',
        },
        {
          id: 'showcase-segment-2',
          text: 'Макет готов, осталось проверить поведение на длинной стенограмме.',
          timestamp: 19,
          recognizedAt: '10:25',
          speaker_id: 1,
          reaction: 'scared',
        },
        {
          id: 'showcase-segment-3',
          text: 'После проверки можно отдавать обновление пользователям.',
          timestamp: 31,
          recognizedAt: '10:26',
          speaker_id: 2,
          reaction: 'happy',
        },
      ],
      speakersById: new Map([
        [0, 'Анна'],
        [1, 'Михаил'],
        [2, 'София'],
      ]),
      selfSpeakerIds: new Set([0]),
      isRecording: false,
      disableAutoScroll: true,
    };
  }

  return common;
}

function ShowcaseProviders({ children }: { children: ReactNode }) {
  const noop = () => undefined;
  const onboardingValue: OnboardingContextType = {
    currentStep: 1,
    totalSteps: 3,
    isMac: true,
    statusLoaded: true,
    shouldRun: true,
    modelPack: {
      status: 'ready',
      percent: 100,
      downloadedMb: 1021,
      totalMb: 1021,
      transcriptionReady: true,
      speakersReady: true,
    },
    transcriptionLabel: 'GigaAM v3',
    summaryModel: 'qwen3:4b',
    setSummaryModel: noop,
    permissions: {
      microphone: 'authorized',
      systemAudio: 'authorized',
      screenRecording: 'authorized',
    },
    permissionsSkipped: false,
    goToStep: noop,
    goNext: noop,
    goPrevious: noop,
    setPermissionStatus: noop,
    setPermissionsSkipped: noop,
    startModelPack: async () => undefined,
    completeOnboarding: async () => 'showcase-meeting',
  };
  const sidebarValue: SidebarContextType = {
    currentMeeting: { id: 'showcase-meeting', title: 'Еженедельный синк' },
    setCurrentMeeting: noop,
    sidebarItems: [],
    isCollapsed: false,
    toggleCollapse: noop,
    sidebarWidth: 232,
    setSidebarWidth: noop,
    isSidebarResizing: false,
    setIsSidebarResizing: noop,
    meetings: [],
    setMeetings: noop,
    isMeetingActive: false,
    setIsMeetingActive: noop,
    handleRecordingToggle: noop,
    searchTranscripts: async () => undefined,
    searchResults: [],
    isSearching: false,
    setServerAddress: noop,
    serverAddress: '',
    transcriptServerAddress: '',
    setTranscriptServerAddress: noop,
    activeSummaryPolls: new Map(),
    startSummaryPolling: noop,
    stopSummaryPolling: noop,
    refetchMeetings: async () => undefined,
  };

  return (
    <TooltipProvider>
      <SidebarContext.Provider value={sidebarValue}>
        <OnboardingContext.Provider value={onboardingValue}>{children}</OnboardingContext.Provider>
      </SidebarContext.Provider>
    </TooltipProvider>
  );
}

function AccordionSpecimen({ module }: { module: ProductionModule }) {
  const Root = module.Accordion as ComponentType<Record<string, unknown>>;
  const Item = module.AccordionItem as ComponentType<Record<string, unknown>>;
  const Trigger = module.AccordionTrigger as ComponentType<Record<string, unknown>>;
  const Content = module.AccordionContent as ComponentType<Record<string, unknown>>;

  return (
    <Root type="single" defaultValue="showcase" collapsible>
      <Item value="showcase">
        <Trigger>Что входит в локальную обработку?</Trigger>
        <Content>Запись, расшифровка и подготовка итогов встречи.</Content>
      </Item>
    </Root>
  );
}

export function ModuleShowcase({ module, title }: { module: ProductionModule; title: string }) {
  const exports = Object.entries(module).filter(([name, value]) =>
    /^[A-Z]/.test(name) && isRenderable(value),
  ) as Array<[string, ComponentType<Record<string, unknown>>]>;

  return (
    <main className="min-h-screen bg-background p-8 text-foreground">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
        {title.startsWith('ui-') || title.startsWith('ui/') ? (
          <PrimitiveModuleShowcase module={module} title={title} />
        ) : exports.length === 0 ? (
          <p className="text-sm text-muted-foreground">В модуле нет отдельного визуального export.</p>
        ) : exports.map(([name, ExportedComponent]) => (
          <section key={name} className="rounded-[var(--ui-radius-20)] bg-[var(--elevation-1)] p-[var(--ui-space-24)]">
            <p className="mb-4 text-xs text-muted-foreground">{name}</p>
            <SpecimenBoundary name={name}>
              <ShowcaseProviders>
                <ExportedComponent {...defaultProps(name)} />
              </ShowcaseProviders>
            </SpecimenBoundary>
          </section>
        ))}
      </div>
    </main>
  );
}
