"use client";

import { type ComponentProps, useEffect, useMemo, useRef, useState } from 'react';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { EmptyStateSummary } from '@/components/EmptyStateSummary';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Copy, Loader2, RefreshCw, Save, Send } from '@/components/memento/LucideCompat';
import { ChatMarkdown } from '@/components/chat/ChatMarkdown';
import { summaryToMarkdown, splitSummaryLead } from '@/lib/summaryToMarkdown';
import { useT } from '@/lib/i18n';

/**
 * Summary for the meeting conversation (variant 3a): flows as content in the feed —
 * NO card, no border, no background. A quiet eyebrow + neutral type selector, a
 * prominent TL;DR lead, and "Ключевые решения / Задачи" behind a "Подробнее"
 * disclosure (collapsed by default). Gold is muted here — the type selector and
 * actions are neutral. The real summary content is converted to Markdown and
 * rendered read-first; picking a template regenerates it.
 */

type SummaryPanelProps = ComponentProps<typeof SummaryPanel>;

interface SummaryMessageProps {
  summaryPanelProps: SummaryPanelProps;
}

export function SummaryMessage({ summaryPanelProps: p }: SummaryMessageProps) {
  const t = useT();
  const [typeOpen, setTypeOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const hasSummary = !!p.aiSummary;
  const hasModel = p.modelConfig.provider !== null && p.modelConfig.model !== null;
  const selectedName =
    p.availableTemplates.find((tp) => tp.id === p.selectedTemplate)?.name ?? t('Summary type');

  const { lead, details } = useMemo(() => {
    if (!hasSummary) return { lead: '', details: '' };
    return splitSummaryLead(summaryToMarkdown(p.aiSummary));
  }, [hasSummary, p.aiSummary]);

  // Pick a template → generate with it (set, then generate once it has changed).
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
    <div>
      {/* Eyebrow + neutral type selector */}
      <div className="mb-3 flex items-center gap-2.5">
        <span className="text-[11px] font-bold uppercase tracking-[0.14em] text-[var(--fg3)]">{t('Summary')}</span>
        <div className="flex-1" />
        <Popover open={typeOpen} onOpenChange={setTypeOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={isGenerating}
              title={t('Summary type')}
              className="inline-flex h-[30px] items-center gap-[7px] rounded-lg pl-2.5 pr-1.5 text-[12.5px] font-semibold text-[var(--fg2)] transition-colors hover:bg-[var(--state-hover-bg)] hover:text-[var(--fg1)] disabled:opacity-50"
            >
              <span className="max-w-[200px] truncate">{selectedName}</span>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
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
        <div className="flex items-center gap-2 py-4 text-sm text-[var(--fg2)]">
          <Loader2 className="h-4 w-4 animate-spin text-[var(--gold)]" />
          {p.getSummaryStatusMessage(p.summaryStatus)}
        </div>
      ) : hasSummary ? (
        <>
          {lead && <p className="m-0 text-[17px] font-medium leading-[1.6] text-[var(--fg1)]">{lead}</p>}

          {details && (
            <>
              <button
                type="button"
                onClick={() => setDetailsOpen((v) => !v)}
                aria-expanded={detailsOpen}
                className="mt-3.5 inline-flex items-center gap-1.5 text-[12.5px] font-semibold text-[var(--fg3)] transition-colors hover:text-[var(--fg1)]"
              >
                <span>{detailsOpen ? t('Collapse details') : t('More — decisions and tasks')}</span>
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d={detailsOpen ? 'm6 15 6-6 6 6' : 'm6 9 6 6 6-6'} />
                </svg>
              </button>
              {detailsOpen && <ChatMarkdown className="mt-4" content={details} />}
            </>
          )}

          {/* Quiet actions */}
          <div className="mt-[18px] flex gap-4">
            <QuietAction icon={<RefreshCw size={14} />} label={t('Regenerate')} onClick={() => void p.onRegenerateSummary()} />
            <QuietAction icon={<Copy size={14} />} label={t('Copy')} onClick={() => void p.onCopySummary()} />
            {p.canShareToTelegram && p.onShareSummaryToTelegram && (
              <QuietAction
                icon={p.isSharingToTelegram ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
                label={t('Telegram')}
                disabled={p.isSharingToTelegram}
                onClick={() => void p.onShareSummaryToTelegram?.()}
              />
            )}
            <QuietAction icon={<Save size={14} />} label={t('Save to note')} onClick={() => void p.onSaveAll()} />
          </div>
        </>
      ) : p.summaryLoadStatus === 'loading' ? (
        <div className="flex items-center gap-2 py-4 text-sm text-[var(--fg2)]">
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

function QuietAction({
  icon,
  label,
  onClick,
  disabled = false,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="inline-flex items-center gap-1.5 text-[12.5px] font-semibold text-[var(--fg3)] transition-colors hover:text-[var(--fg1)] disabled:opacity-50"
    >
      {icon}
      {label}
    </button>
  );
}
