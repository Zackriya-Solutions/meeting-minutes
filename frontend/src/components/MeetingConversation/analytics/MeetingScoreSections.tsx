"use client";

import { useT } from '@/lib/i18n';
import { ChatMarkdown } from '@/components/chat/ChatMarkdown';
import type { MeetingAnalyticsSections } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import { SectionHeading } from './primitives';

/**
 * The analytical «Что мешало» section rendered after the summary. Score and agenda
 * coverage stay out of the product UI for now.
 */

export function MeetingScoreSections({
  sections,
}: {
  sections: MeetingAnalyticsSections;
}) {
  const t = useT();
  const { what_hindered: whatHindered } = sections;

  if (whatHindered.length === 0) return null;

  return (
    <section>
      <SectionHeading>{t('What hindered')}</SectionHeading>
      <ChatMarkdown
        content={whatHindered.map((item) => `- ${item}`).join('\n')}
        className="mm-summary-list mm-summary-plain mt-2"
      />
    </section>
  );
}
