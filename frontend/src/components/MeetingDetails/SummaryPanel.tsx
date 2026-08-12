"use client";

import { Summary, SummaryResponse, Transcript } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { BlockNoteSummaryView, BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { EmptyStateSummary } from '@/components/EmptyStateSummary';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SummaryGeneratorButtonGroup } from './SummaryGeneratorButtonGroup';
import { SummaryUpdaterButtonGroup } from './SummaryUpdaterButtonGroup';
import { MeetingContentWindowNotice } from './MeetingContentWindowNotice';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';
import { useEffect, useRef, useState, RefObject } from 'react';
import { toast } from 'sonner';
import { AlertTriangle, Languages, ChevronDown, RefreshCw, Sparkles, X } from '@/components/deslop-icons';
import type { VisibleTemplateSuggestion } from '@/hooks/meeting-details/useTemplates';
import { Button } from '@/components/ui/button';
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { useRecentLanguages } from '@/hooks/useRecentLanguages';
import { labelForCode } from '@/lib/summary-languages';
import { StandupWorkflowPanel } from './StandupWorkflowPanel';
import { InterviewWorkflowPanel } from './InterviewWorkflowPanel';
import { OneOnOneWorkflowPanel } from './OneOnOneWorkflowPanel';
import {
  readMeetingSummaryLanguage,
  saveMeetingSummaryLanguage,
  SummaryLanguageStorage,
} from '@/lib/summary-language-preferences';

interface SummaryPanelProps {
  meeting: {
    id: string;
    title: string;
    created_at: string;
  };
  meetingTitle: string;
  onTitleChange: (title: string) => void;
  isEditingTitle: boolean;
  onStartEditTitle: () => void;
  onFinishEditTitle: () => void;
  isTitleDirty: boolean;
  summaryRef: RefObject<BlockNoteSummaryViewRef>;
  isSaving: boolean;
  onSaveAll: () => Promise<void>;
  onCopySummary: () => Promise<void>;
  /** Opens Telegram's chat picker with the summary prefilled. Absent when unavailable. */
  onShareSummaryToTelegram?: () => Promise<void>;
  /** False in local-only mode, which hides every Telegram affordance. */
  canShareToTelegram?: boolean;
  isSharingToTelegram?: boolean;
  onOpenFolder: () => Promise<void>;
  onDiscussSummary: () => void;
  aiSummary: Summary | null;
  summaryLoadStatus?: 'loading' | 'loaded' | 'absent' | 'error';
  summaryLoadError?: string | null;
  onRetrySummary?: () => Promise<void> | void;
  speakerAttributionStale?: boolean;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  transcripts: Transcript[];
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  onStopGeneration: () => void;
  customPrompt: string;
  summaryResponse: SummaryResponse | null;
  onSaveSummary: (summary: Summary | { markdown?: string; summary_json?: any[] }) => Promise<void>;
  onSummaryChange: (summary: Summary) => void;
  onDirtyChange: (isDirty: boolean) => void;
  summaryError: string | null;
  onRegenerateSummary: () => Promise<void>;
  getSummaryStatusMessage: (status: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error') => string;
  availableTemplates: Array<{ id: string, name: string, description: string }>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  templateSuggestion?: VisibleTemplateSuggestion | null;
  onDismissTemplateSuggestion?: () => void;
  isModelConfigLoading?: boolean;
  onOpenModelSettings?: (openFn: () => void) => void;
}

export function SummaryPanel({
  meeting,
  meetingTitle,
  onTitleChange,
  isEditingTitle,
  onStartEditTitle,
  onFinishEditTitle,
  isTitleDirty,
  summaryRef,
  isSaving,
  onSaveAll,
  onCopySummary,
  onOpenFolder,
  onDiscussSummary,
  aiSummary,
  summaryLoadStatus = 'loaded',
  summaryLoadError = null,
  onRetrySummary,
  speakerAttributionStale = false,
  summaryStatus,
  transcripts,
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onGenerateSummary,
  onStopGeneration,
  customPrompt,
  summaryResponse,
  onSaveSummary,
  onSummaryChange,
  onDirtyChange,
  summaryError,
  onRegenerateSummary,
  getSummaryStatusMessage,
  availableTemplates,
  selectedTemplate,
  onTemplateSelect,
  templateSuggestion,
  onDismissTemplateSuggestion,
  isModelConfigLoading = false,
  onOpenModelSettings
}: SummaryPanelProps) {
  const t = useT();
  const [summaryLang, setSummaryLang] = useState<string | null>(null);
  const [summaryLangStorage, setSummaryLangStorage] = useState<SummaryLanguageStorage>('metadata');
  const [langPickerOpen, setLangPickerOpen] = useState(false);
  const languageLoadVersionRef = useRef(0);
  const activeMeetingIdRef = useRef(meeting.id);
  const languageSaveVersionRef = useRef(0);
  const languageSaveLoopRunningRef = useRef(false);
  const latestLanguageSaveRequestRef = useRef<{
    version: number;
    meetingId: string;
    language: string | null;
    rollback: {
      language: string | null;
      storage: SummaryLanguageStorage;
    };
  } | null>(null);
  activeMeetingIdRef.current = meeting.id;
  const { addRecent } = useRecentLanguages();

  const effectiveLangLabel = summaryLang ? labelForCode(summaryLang) : t('Auto');
  const isLocalFallbackLanguage = summaryLangStorage === 'local_fallback';
  const autoSubtitle = isLocalFallbackLanguage
    ? t('Saved on this device for folderless meetings')
    : t('Uses dominant transcript language');

  useEffect(() => {
    let cancelled = false;
    const loadVersion = languageLoadVersionRef.current + 1;
    languageLoadVersionRef.current = loadVersion;

    const loadSummaryLanguage = async () => {
      try {
        const stored = await readMeetingSummaryLanguage(meeting.id);
        if (!cancelled && languageLoadVersionRef.current === loadVersion) {
          setSummaryLang(stored.language);
          setSummaryLangStorage(stored.storage);
        }
      } catch (err) {
        console.error('Failed to load summary language:', err);
        toast.warning(t('Could not load saved summary language'), {
          description: t('Using Auto until meeting metadata can be read.'),
        });
        if (!cancelled && languageLoadVersionRef.current === loadVersion) setSummaryLang(null);
      }
    };

    loadSummaryLanguage();

    return () => {
      cancelled = true;
    };
  }, [meeting.id]);

  const persistLatestLanguageSelection = async () => {
    if (languageSaveLoopRunningRef.current) return;
    languageSaveLoopRunningRef.current = true;

    try {
      while (true) {
        const request = latestLanguageSaveRequestRef.current;
        if (!request) return;

        try {
          const saved = await saveMeetingSummaryLanguage(request.meetingId, request.language);
          const latest = latestLanguageSaveRequestRef.current;
          if (
            latest?.version === request.version &&
            activeMeetingIdRef.current === request.meetingId
          ) {
            setSummaryLang(saved.language);
            setSummaryLangStorage(saved.storage);
            if (saved.storage === 'local_fallback') {
              toast.info(t('Summary language saved on this device'), {
                description: t('This meeting has no recording folder, so the preference cannot be written to meeting metadata.'),
              });
            }
            if (request.language) {
              addRecent(request.language);
            }
            return;
          }

          if (latest?.version === request.version) return;
        } catch (err) {
          const latest = latestLanguageSaveRequestRef.current;
          if (
            latest?.version === request.version &&
            activeMeetingIdRef.current === request.meetingId
          ) {
            console.error('Failed to persist summary language:', err);
            toast.error(t('Failed to save summary language'));
            setSummaryLang(request.rollback.language);
            setSummaryLangStorage(request.rollback.storage);
            return;
          }

          console.warn('Ignoring failed stale summary language save:', err);
          if (latest?.version === request.version) return;
        }
      }
    } finally {
      languageSaveLoopRunningRef.current = false;
    }
  };

  const handleLangChange = (code: string | null) => {
    const previous = summaryLang;
    const previousStorage = summaryLangStorage;
    const nextStored = code;
    languageLoadVersionRef.current += 1;
    latestLanguageSaveRequestRef.current = {
      version: languageSaveVersionRef.current + 1,
      meetingId: meeting.id,
      language: nextStored,
      rollback: {
        language: previous,
        storage: previousStorage,
      },
    };
    languageSaveVersionRef.current += 1;
    setSummaryLang(nextStored);
    setLangPickerOpen(false);
    void persistLatestLanguageSelection();
  };

  const isSummaryLoading = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

  const languageSlot = (
    <Popover open={langPickerOpen} onOpenChange={setLangPickerOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          title={`${t('Summary language')}: ${effectiveLangLabel}${isLocalFallbackLanguage ? ` (${t('saved on this device')})` : ''}`}
          aria-label={t('Set summary language')}
        >
          <Languages size={18} />
          <span className="hidden lg:inline">{effectiveLangLabel}</span>
          <ChevronDown size={14} className="text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-auto p-0 border-0 shadow-none bg-transparent"
      >
        <LanguagePickerPopover
          value={summaryLang}
          onChange={handleLangChange}
          autoSubtitle={autoSubtitle}
        />
      </PopoverContent>
    </Popover>
  );

  return (
    <div className="flex min-w-0 flex-1 flex-col overflow-hidden bg-background">
      {/* Title area */}
      <div className="border-b border-border p-4">
        {/* <EditableTitle
          title={meetingTitle}
          isEditing={isEditingTitle}
          onStartEditing={onStartEditTitle}
          onFinishEditing={onFinishEditTitle}
          onChange={onTitleChange}
        /> */}

        {/* Button groups - only show when summary exists. flex-wrap: the labels
            show/hide on WINDOW (lg:) breakpoints, not pane width, so on a wide
            window with a narrow summary pane the two shrink-0 groups can exceed
            the pane — without wrapping they clip at both edges until the user
            drags the splitter. */}
        {aiSummary && !isSummaryLoading && (
          <div className="flex w-full flex-col items-center gap-2 pt-0">
            <div className="flex flex-wrap items-center justify-center gap-2">
            {/* Left-aligned: Summary Generator Button Group */}
            <div className="flex-shrink-0">
              <SummaryGeneratorButtonGroup
                modelConfig={modelConfig}
                setModelConfig={setModelConfig}
                onSaveModelConfig={onSaveModelConfig}
                onGenerateSummary={onGenerateSummary}
                onStopGeneration={onStopGeneration}
                customPrompt={customPrompt}
                summaryStatus={summaryStatus}
                availableTemplates={availableTemplates}
                selectedTemplate={selectedTemplate}
                onTemplateSelect={onTemplateSelect}
                hasTranscripts={transcripts.length > 0}
                hasSummary={!!aiSummary}
                isModelConfigLoading={isModelConfigLoading}
                onOpenModelSettings={onOpenModelSettings}
                languageSlot={languageSlot}
              />
            </div>

            {/* Right-aligned: Summary Updater Button Group */}
            <div className="flex-shrink-0">
              <SummaryUpdaterButtonGroup
                isSaving={isSaving}
                isDirty={isTitleDirty || (summaryRef.current?.isDirty || false)}
                onSave={onSaveAll}
                onCopy={onCopySummary}
                onFind={() => {
                  // TODO: Implement find in summary functionality
                  console.log('Find in summary clicked');
                }}
                onOpenFolder={onOpenFolder}
                onDiscuss={onDiscussSummary}
                hasSummary={!!aiSummary}
              />
            </div>
            </div>
            <p className="text-xs text-muted-foreground">
              {t('Saved automatically. The Save button becomes available after manual edits.')}
            </p>
          </div>
        )}
      </div>

      {transcripts.length > 0 && !isSummaryLoading && (
        <MeetingContentWindowNotice meetingId={meeting.id} />
      )}
      {aiSummary && speakerAttributionStale && !isSummaryLoading && (
        <div className="mx-4 mt-3 flex flex-wrap items-center gap-3 rounded-lg border border-primary/40 bg-primary/10 px-4 py-3">
          <AlertTriangle className="h-5 w-5 shrink-0 text-primary" />
          <div className="min-w-[220px] flex-1">
            <p className="text-sm font-semibold text-foreground">
              {t('Speaker names changed after this summary was created')}
            </p>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {t('The existing text is kept to protect manual edits. Regenerate the summary to use the current speaker names and attribution.')}
            </p>
          </div>
          <Button
            type="button"
            size="sm"
            onClick={() => void onRegenerateSummary()}
            className="shrink-0 bg-primary text-black hover:bg-primary/90"
          >
            <RefreshCw className="h-4 w-4" />
            {t('Regenerate with current names')}
          </Button>
        </div>
      )}
      {aiSummary && summaryLoadError && !isSummaryLoading && (
        <div className="mx-4 mt-3 flex flex-wrap items-center gap-3 rounded-lg border border-primary/40 bg-primary/10 px-4 py-3">
          <AlertTriangle className="h-5 w-5 shrink-0 text-primary" />
          <p className="min-w-[220px] flex-1 text-xs text-muted-foreground">
            {t('Could not verify the saved summary. The last loaded version is still shown.')}
          </p>
          {onRetrySummary && (
            <Button type="button" size="sm" variant="outline" onClick={() => void onRetrySummary()}>
              <RefreshCw className="h-4 w-4" />
              {t('Retry loading summary')}
            </Button>
          )}
        </div>
      )}
      {templateSuggestion && selectedTemplate !== 'daily_standup' && !isSummaryLoading && (
        <div className="mx-4 mt-3 flex items-center gap-3 rounded-lg border border-primary/40 bg-primary/10 px-4 py-3">
          <Sparkles className="h-5 w-5 shrink-0 text-primary" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold text-foreground">{templateSuggestion.title}</p>
            <p className="mt-0.5 text-xs text-muted-foreground">{templateSuggestion.description}</p>
          </div>
          <Button
            type="button"
            size="sm"
            onClick={() => onTemplateSelect('daily_standup', 'Daily Standup')}
            className="shrink-0 bg-primary text-black hover:bg-primary/90"
          >
            {t('Use Standup V2')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={onDismissTemplateSuggestion}
            aria-label={t('Dismiss')}
            title={t('Dismiss')}
            className="h-8 w-8 shrink-0 p-0"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      )}
      <StandupWorkflowPanel
        meetingId={meeting.id}
        summaryStatus={summaryStatus}
        standupSelected={selectedTemplate === 'daily_standup'}
      />
      <InterviewWorkflowPanel
        meetingId={meeting.id}
        summaryStatus={summaryStatus}
        interviewSelected={selectedTemplate === 'interview_memory'}
      />
      <OneOnOneWorkflowPanel
        meetingId={meeting.id}
        summaryStatus={summaryStatus}
        oneOnOneSelected={selectedTemplate === 'one_on_one'}
      />

      {isSummaryLoading ? (
        <div className="flex flex-col h-full">
          {/* Show button group during generation */}
          <div className="flex items-center justify-center pt-8 pb-4">
            <SummaryGeneratorButtonGroup
              modelConfig={modelConfig}
              setModelConfig={setModelConfig}
              onSaveModelConfig={onSaveModelConfig}
              onGenerateSummary={onGenerateSummary}
              onStopGeneration={onStopGeneration}
              customPrompt={customPrompt}
              summaryStatus={summaryStatus}
              availableTemplates={availableTemplates}
              selectedTemplate={selectedTemplate}
              onTemplateSelect={onTemplateSelect}
              hasTranscripts={transcripts.length > 0}
              isModelConfigLoading={isModelConfigLoading}
              onOpenModelSettings={onOpenModelSettings}
            />
          </div>
          {/* Loading spinner */}
          <div className="flex items-center justify-center flex-1">
            <div className="text-center">
              <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-primary/40 mb-4"></div>
              <p className="text-muted-foreground">{t('Generating AI Summary...')}</p>
            </div>
          </div>
        </div>
      ) : !aiSummary ? (
        <div className="flex flex-col h-full">
          {summaryLoadStatus === 'loading' ? (
            <div className="flex flex-1 items-center justify-center">
              <div className="text-center text-muted-foreground">
                <RefreshCw className="mx-auto mb-3 h-7 w-7 animate-spin text-primary" />
                <p className="text-sm font-medium">{t('Loading saved summary...')}</p>
              </div>
            </div>
          ) : summaryLoadError ? (
            <div className="flex flex-1 items-center justify-center p-6">
              <div className="max-w-md rounded-lg border border-primary/40 bg-primary/10 p-5 text-center">
                <AlertTriangle className="mx-auto mb-3 h-7 w-7 text-primary" />
                <h3 className="text-base font-semibold text-foreground">
                  {t('The saved summary could not be loaded')}
                </h3>
                <p className="mt-2 text-sm text-muted-foreground">
                  {t('The summary was not deleted. Retry loading it instead of creating a replacement.')}
                </p>
                <p className="mt-2 break-words text-xs text-muted-foreground">{summaryLoadError}</p>
                {onRetrySummary && (
                  <Button type="button" className="mt-4" onClick={() => void onRetrySummary()}>
                    <RefreshCw className="h-4 w-4" />
                    {t('Retry loading summary')}
                  </Button>
                )}
              </div>
            </div>
          ) : (
            <>
              {/* Centered Summary Generator Button Group only after the backend explicitly says absent. */}
              <div className="flex items-center justify-center gap-2 pt-8 pb-4">
                <SummaryGeneratorButtonGroup
                  modelConfig={modelConfig}
                  setModelConfig={setModelConfig}
                  onSaveModelConfig={onSaveModelConfig}
                  onGenerateSummary={onGenerateSummary}
                  onStopGeneration={onStopGeneration}
                  customPrompt={customPrompt}
                  summaryStatus={summaryStatus}
                  availableTemplates={availableTemplates}
                  selectedTemplate={selectedTemplate}
                  onTemplateSelect={onTemplateSelect}
                  hasTranscripts={transcripts.length > 0}
                  hasSummary={false}
                  isModelConfigLoading={isModelConfigLoading}
                  onOpenModelSettings={onOpenModelSettings}
                  languageSlot={transcripts.length > 0 ? languageSlot : undefined}
                />
              </div>
              <EmptyStateSummary
                onGenerate={() => onGenerateSummary(customPrompt)}
                hasModel={modelConfig.provider !== null && modelConfig.model !== null}
                isGenerating={isSummaryLoading}
              />
            </>
          )}
        </div>
      ) : transcripts?.length > 0 && (
        <div className="flex-1 overflow-y-auto min-h-0">
          {summaryResponse && (
            <div className="fixed bottom-0 left-0 right-0 bg-background shadow-none p-4 max-h-1/3 overflow-y-auto">
              <h3 className="text-lg font-semibold mb-2">{t('Meeting Summary')}</h3>
              <div className="grid grid-cols-2 gap-4">
                <div className="bg-background p-4 rounded-lg shadow-none">
                  <h4 className="font-medium mb-1">{t('Key Points')}</h4>
                  <ul className="list-disc pl-4">
                    {summaryResponse.summary.key_points.blocks.map((block, i) => (
                      <li key={i} className="text-sm">{block.content}</li>
                    ))}
                  </ul>
                </div>
                <div className="bg-background p-4 rounded-lg shadow-none mt-4">
                  <h4 className="font-medium mb-1">{t('Action Items')}</h4>
                  <ul className="list-disc pl-4">
                    {summaryResponse.summary.action_items.blocks.map((block, i) => (
                      <li key={i} className="text-sm">{block.content}</li>
                    ))}
                  </ul>
                </div>
                <div className="bg-background p-4 rounded-lg shadow-none mt-4">
                  <h4 className="font-medium mb-1">{t('Decisions')}</h4>
                  <ul className="list-disc pl-4">
                    {summaryResponse.summary.decisions.blocks.map((block, i) => (
                      <li key={i} className="text-sm">{block.content}</li>
                    ))}
                  </ul>
                </div>
                <div className="bg-background p-4 rounded-lg shadow-none mt-4">
                  <h4 className="font-medium mb-1">{t('Main Topics')}</h4>
                  <ul className="list-disc pl-4">
                    {summaryResponse.summary.main_topics.blocks.map((block, i) => (
                      <li key={i} className="text-sm">{block.content}</li>
                    ))}
                  </ul>
                </div>
              </div>
              {summaryResponse.raw_summary ? (
                <div className="mt-4">
                  <h4 className="font-medium mb-1">{t('Full Summary')}</h4>
                  <p className="text-sm whitespace-pre-wrap">{summaryResponse.raw_summary}</p>
                </div>
              ) : null}
            </div>
          )}
          <div className="p-6 w-full">
            <BlockNoteSummaryView
              ref={summaryRef}
              summaryData={aiSummary}
              onSave={onSaveSummary}
              onSummaryChange={onSummaryChange}
              onDirtyChange={onDirtyChange}
              status={summaryStatus}
              error={summaryError}
              onRegenerateSummary={() => {
                Analytics.trackButtonClick('regenerate_summary', 'meeting_details');
                onRegenerateSummary();
              }}
              meeting={{
                id: meeting.id,
                title: meetingTitle,
                created_at: meeting.created_at
              }}
            />
          </div>
          {summaryStatus !== 'idle' && (
            <div className={`mt-4 p-4 rounded-lg ${summaryStatus === 'error' ? 'bg-destructive/10 text-destructive' :
              summaryStatus === 'completed' ? 'bg-success/10 text-success' :
                'bg-primary/10 text-primary'
              }`}>
              <p className="text-sm font-medium">{getSummaryStatusMessage(summaryStatus)}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
