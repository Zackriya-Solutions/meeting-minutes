"use client";

import { type ComponentProps, useMemo } from 'react';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { Loader2 } from '@/components/deslop-icons';
import { ChatMarkdown } from '@/components/chat/ChatMarkdown';
import {
  extractSummaryAgreements,
  extractSummaryParticipants,
  summaryToMarkdown,
  splitSummaryLead,
} from '@/lib/summaryToMarkdown';
import { useT } from '@/lib/i18n';

/**
 * Summary for the meeting conversation (variant 3a): flows as content in the feed —
 * The complete overview is rendered first, followed by a bounded agreements block.
 * Picking a template regenerates both from the same persisted report.
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

  const content = useMemo(() => {
    if (!hasSummary) return { participants: '', lead: '', agreements: '' };
    const markdown = summaryToMarkdown(p.aiSummary);
    return {
      participants: extractSummaryParticipants(markdown),
      lead: splitSummaryLead(markdown).lead,
      agreements: extractSummaryAgreements(markdown),
    };
  }, [hasSummary, p.aiSummary]);

  return (
    <div>
      {isGenerating ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin text-primary" />
          {p.getSummaryStatusMessage(p.summaryStatus)}
        </div>
      ) : hasSummary ? (
        <>
          {content.participants ? (
            <section>
              <h2 className="text-sm font-semibold leading-5 text-[var(--primary-50)]">{t('Participants')}</h2>
              <ChatMarkdown content={content.participants} className="mm-summary-list mt-2" />
            </section>
          ) : null}
          {content.lead ? (
            <section className={content.participants ? 'mt-6' : undefined}>
              <h2 className="text-sm font-semibold leading-5 text-[var(--primary-50)]">{t('About the meeting')}</h2>
              <ChatMarkdown content={content.lead} className="mm-summary-list mt-2" />
            </section>
          ) : null}
          {content.agreements ? (
            <section className="mt-6">
              <h2 className="text-sm font-semibold leading-5 text-[var(--primary-50)]">{t('Agreements')}</h2>
              <ChatMarkdown
                content={content.agreements}
                className="mm-summary-list mt-2"
              />
            </section>
          ) : null}
        </>
      ) : p.summaryLoadStatus === 'loading' ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin text-primary" />
          {t('Loading saved summary...')}
        </div>
      ) : null}

      {p.summaryError && !isGenerating && hasSummary && (
        <p className="mt-2 text-xs text-destructive">{p.summaryError}</p>
      )}
    </div>
  );
}
