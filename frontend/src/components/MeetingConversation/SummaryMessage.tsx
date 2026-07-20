"use client";

import { type ComponentProps } from 'react';
import { motion } from 'framer-motion';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';

/**
 * Summary as the first assistant message of the meeting conversation (variant 2a).
 *
 * `SummaryPanel` no longer renders a title (its EditableTitle is disabled — the
 * title lives in the screen top bar now), so it is reused wholesale here: it
 * already hosts the type selector + generate/stop + language + model settings
 * (SummaryGeneratorButtonGroup), the save/copy/discuss actions
 * (SummaryUpdaterButtonGroup), the BlockNote body, and every summary state
 * (loading / absent / generating / error). We wrap it in a bounded, message-styled
 * card so it reads as the pinned first message while its long body scrolls in
 * place instead of pushing the chat thread down.
 */

interface SummaryMessageProps {
  summaryPanelProps: ComponentProps<typeof SummaryPanel>;
}

export function SummaryMessage({ summaryPanelProps }: SummaryMessageProps) {
  const t = useT();

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
      <div className="min-w-0 flex-1">
        <div className="mb-1.5 flex items-center gap-2">
          <span className="text-sm font-semibold text-[var(--fg1)]">{t('Meeting summary')}</span>
        </div>
        <div className="flex h-[min(68vh,680px)] flex-col overflow-hidden rounded-[var(--radius-16)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)]">
          <SummaryPanel {...summaryPanelProps} />
        </div>
      </div>
    </motion.div>
  );
}
