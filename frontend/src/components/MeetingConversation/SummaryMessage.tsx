"use client";

import { type ComponentProps, useEffect, useRef, useState } from 'react';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { BlockNoteSummaryView } from '@/components/AISummary/BlockNoteSummaryView';
import { EmptyStateSummary } from '@/components/EmptyStateSummary';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Copy, Loader2, RefreshCw, Save } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

/**
 * Summary result card for the meeting conversation (variant 2a), matching the
 * delivery prototype: a bordered `bg-surface` card with "Саммари встречи", a gold
 * pill type selector (opens a template dropdown; choosing a type regenerates), the
 * summary body (BlockNoteSummaryView — real content), and a Пересобрать / Копировать
 * / В заметку action row. Language & model live in the screen "⋯" menu.
 */

type SummaryPanelProps = ComponentProps<typeof SummaryPanel>;

interface SummaryMessageProps {
  summaryPanelProps: SummaryPanelProps;
}

export function SummaryMessage({ summaryPanelProps: p }: SummaryMessageProps) {
  const t = useT();
  const [typeOpen, setTypeOpen] = useState(false);

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const hasSummary = !!p.aiSummary;
  const hasModel = p.modelConfig.provider !== null && p.modelConfig.model !== null;
  const selectedName =
    p.availableTemplates.find((tp) => tp.id === p.selectedTemplate)?.name ?? t('Summary type');

  // Pick a template → generate with it. Generation reads selectedTemplate from a
  // closure, so we set it, then generate once it has actually changed (effect).
  const pendingTemplateRef = useRef<string | null>(null);
  const pickTemplate = (id: string, name: string) => {
    setTypeOpen(false);
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

  return (
    <div className="rounded-[16px] border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-5 py-[18px]">
      {/* Header: title + gold pill type selector */}
      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <span className="text-sm font-semibold text-[var(--fg1)]">{t('Meeting summary')}</span>
        <div className="flex-1" />
        <Popover open={typeOpen} onOpenChange={setTypeOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={isGenerating}
              title={t('Summary type')}
              className="inline-flex h-[30px] items-center gap-[7px] rounded-full border border-[var(--gold-border)] bg-[var(--gold-soft)] px-2.5 text-xs font-semibold text-[var(--gold)] transition-colors hover:bg-[var(--gold-soft-strong)] disabled:opacity-50"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
                <path d="M5 7h14M5 12h14M5 17h8" />
              </svg>
              <span className="max-w-[180px] truncate">{selectedName}</span>
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-[300px] p-1.5">
            <div className="px-2.5 pb-1 pt-2 text-[10px] font-bold uppercase tracking-[0.14em] text-[var(--fg3)]">
              {t('Summary type · rebuilds the message')}
            </div>
            {p.availableTemplates.map((tp) => (
              <button
                key={tp.id}
                type="button"
                onClick={() => pickTemplate(tp.id, tp.name)}
                className="flex w-full items-start gap-2.5 rounded-[10px] px-2.5 py-2 text-left transition-colors hover:bg-[var(--state-hover-bg)]"
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] font-semibold text-[var(--fg1)]">{tp.name}</span>
                  <span className="block text-[11.5px] leading-snug text-[var(--fg3)]">{tp.description}</span>
                </span>
                {tp.id === p.selectedTemplate && (
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--success)" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" className="mt-0.5 shrink-0">
                    <path d="m6 12.5 4 4 8-8.5" />
                  </svg>
                )}
              </button>
            ))}
          </PopoverContent>
        </Popover>
      </div>

      {/* Body */}
      {isGenerating ? (
        <div className="flex items-center gap-2 py-6 text-sm text-[var(--fg2)]">
          <Loader2 className="h-4 w-4 animate-spin text-[var(--gold)]" />
          {p.getSummaryStatusMessage(p.summaryStatus)}
        </div>
      ) : hasSummary ? (
        <>
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
          {/* Actions */}
          <div className="mt-4 flex gap-1.5 border-t border-[var(--border-subtle)] pt-3.5">
            <ActionPill icon={<RefreshCw size={14} />} label={t('Regenerate')} onClick={() => void p.onRegenerateSummary()} />
            <ActionPill icon={<Copy size={14} />} label={t('Copy')} onClick={() => void p.onCopySummary()} />
            <ActionPill icon={<Save size={14} />} label={t('Save to note')} onClick={() => void p.onSaveAll()} />
          </div>
        </>
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
  );
}

function ActionPill({ icon, label, onClick }: { icon: React.ReactNode; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex h-[30px] items-center gap-1.5 rounded-full border border-[var(--border-strong)] px-[11px] text-xs font-semibold text-[var(--fg2)] transition-colors hover:bg-[var(--state-hover-bg)] hover:text-[var(--fg1)]"
    >
      {icon}
      {label}
    </button>
  );
}
