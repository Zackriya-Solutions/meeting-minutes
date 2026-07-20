"use client";

import { type ComponentProps, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { BlockNoteSummaryView } from '@/components/AISummary/BlockNoteSummaryView';
import { EmptyStateSummary } from '@/components/EmptyStateSummary';
import { Icon } from '@/components/memento/Icon';
import { Loader2, RefreshCw } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

/**
 * Summary as the first assistant message of the meeting conversation (variant 2a).
 *
 * Rendered inline like a chat message (avatar + bubble) — NOT as a bounded panel —
 * so the transcript pin, the summary, and the Q&A read as one continuous thread.
 * The summary body reuses `BlockNoteSummaryView` (which flows to its content
 * height); the compact type selector in the header starts/rebuilds the summary
 * (the user drives summarization by picking a template). Secondary actions
 * (copy / save / model / language) live in the screen's "⋯" menu.
 *
 * It receives the same prop bundle the old SummaryPanel took (transport only) and
 * uses the fields it needs — no separate plumbing in page-content.
 */

type SummaryPanelProps = ComponentProps<typeof SummaryPanel>;

interface SummaryMessageProps {
  summaryPanelProps: SummaryPanelProps;
}

export function SummaryMessage({ summaryPanelProps: p }: SummaryMessageProps) {
  const t = useT();

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const hasSummary = !!p.aiSummary;
  const hasModel = p.modelConfig.provider !== null && p.modelConfig.model !== null;

  // "Pick a template → generate with it". Generation reads selectedTemplate from a
  // closure, so we can't select+generate synchronously — set the template, then
  // generate once it has actually changed (via the effect below). Re-picking the
  // current template regenerates immediately.
  const pendingTemplateRef = useRef<string | null>(null);
  const pickTemplate = (id: string, name: string) => {
    if (isGenerating) return;
    if (id === p.selectedTemplate) {
      void p.onGenerateSummary('');
      return;
    }
    pendingTemplateRef.current = id;
    p.onTemplateSelect(id, name);
  };
  useEffect(() => {
    if (pendingTemplateRef.current && pendingTemplateRef.current === p.selectedTemplate) {
      pendingTemplateRef.current = null;
      void p.onGenerateSummary('');
    }
  }, [p.selectedTemplate, p.onGenerateSummary]);

  const typeSelector = (
    <select
      value={p.selectedTemplate}
      disabled={isGenerating}
      onChange={(e) => {
        const tpl = p.availableTemplates.find((x) => x.id === e.target.value);
        if (tpl) pickTemplate(tpl.id, tpl.name);
      }}
      className="mm-select max-w-[220px] text-xs"
      title={t('Summary type')}
      aria-label={t('Summary type')}
    >
      {p.availableTemplates.map((tpl) => (
        <option key={tpl.id} value={tpl.id}>
          {tpl.name}
        </option>
      ))}
    </select>
  );

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      className="flex gap-3"
    >
      <span
        aria-hidden
        className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--gold-soft)] text-[var(--gold)]"
      >
        <Icon name="spark" size={16} />
      </span>
      <div className="min-w-0 flex-1 rounded-2xl bg-[var(--bg-elevated)] px-4 py-3">
        <div className="mb-2 flex items-center gap-2">
          <span className="text-sm font-semibold text-[var(--fg1)]">{t('Meeting summary')}</span>
          <div className="ml-auto flex items-center gap-1.5">
            {typeSelector}
            {hasSummary && !isGenerating && (
              <button
                type="button"
                onClick={() => void p.onRegenerateSummary()}
                className="mm-icon-button mm-hover"
                title={t('Regenerate')}
                aria-label={t('Regenerate')}
              >
                <RefreshCw size={16} />
              </button>
            )}
          </div>
        </div>

        {isGenerating ? (
          <div className="flex items-center gap-2 py-6 text-sm text-[var(--fg2)]">
            <Loader2 className="h-4 w-4 animate-spin text-[var(--gold)]" />
            {p.getSummaryStatusMessage(p.summaryStatus)}
          </div>
        ) : hasSummary ? (
          <BlockNoteSummaryView
            ref={p.summaryRef}
            summaryData={p.aiSummary}
            onSave={p.onSaveSummary}
            onSummaryChange={p.onSummaryChange}
            onDirtyChange={p.onDirtyChange}
            status={p.summaryStatus}
            error={p.summaryError}
            onRegenerateSummary={() => void p.onRegenerateSummary()}
            meeting={{ id: p.meeting.id, title: p.meetingTitle, created_at: p.meeting.created_at }}
          />
        ) : p.summaryLoadStatus === 'loading' ? (
          <div className="flex items-center gap-2 py-6 text-sm text-[var(--fg2)]">
            <Loader2 className="h-4 w-4 animate-spin text-[var(--gold)]" />
            {t('Loading saved summary...')}
          </div>
        ) : (
          <div className="py-1">
            <p className="mb-3 text-sm text-[var(--fg2)]">{t('No summary yet. Choose a type to generate it.')}</p>
            <EmptyStateSummary onGenerate={() => void p.onGenerateSummary('')} hasModel={hasModel} isGenerating={isGenerating} />
          </div>
        )}

        {p.summaryError && !isGenerating && hasSummary && (
          <p className="mt-2 text-xs text-[var(--danger)]">{p.summaryError}</p>
        )}
      </div>
    </motion.div>
  );
}
