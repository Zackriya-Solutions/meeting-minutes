"use client";

import { motion } from 'framer-motion';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';
import type { ChatMessage, Citation, RetrievalDiagnostics } from '@/hooks/useMeetingChat';

/**
 * Presentational chat primitives shared by the standalone archive/collection chat
 * page and the embedded meeting conversation (variant 2a). The citation click
 * handler is injected so each surface can decide what a citation does — the
 * archive page routes to `/meeting-details`, while the meeting screen expands the
 * transcript pin and scrolls to the cited segment in place.
 */

export function MessageBubble({
  msg,
  meetingTitle,
  onCite,
  showMeetingLabel = true,
}: {
  msg: ChatMessage;
  meetingTitle: (id: string) => string;
  onCite: (c: Citation) => void;
  /** Hide the per-citation meeting label when the whole thread is one meeting. */
  showMeetingLabel?: boolean;
}) {
  const t = useT();
  const isUser = msg.role === 'user';
  const notFound = msg.role === 'assistant' && msg.found === false && !msg.error;

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.18 }}
      className={cn('flex', isUser ? 'justify-end' : 'justify-start')}
    >
      <div
        className={cn(
          'max-w-[85%] rounded-2xl px-4 py-2.5 text-sm',
          isUser
            ? 'bg-[var(--gold)] text-[var(--fg-inverse)]'
            : msg.error
              ? 'border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] text-[var(--danger)]'
              : notFound
                ? 'border border-[var(--border-subtle)] bg-[var(--bg-sheet)] text-[var(--fg2)]'
                : 'bg-[var(--bg-elevated)] text-[var(--fg1)]',
        )}
      >
        {notFound && (
          <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-[var(--fg2)]">
            <Icon name="search" size={14} />
            {t('Not found in your meetings')}
          </div>
        )}
        <div className="whitespace-pre-wrap leading-relaxed">{msg.content}</div>

        {notFound && msg.diagnostics && <RetrievalExplanation diagnostics={msg.diagnostics} />}

        {msg.warning && (
          <div className="mt-2 flex items-center gap-1.5 text-xs text-[var(--gold)]">
            <Icon name="alert" size={14} />
            {msg.warning}
          </div>
        )}

        {!!msg.citations?.length && (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {msg.citations.map((c) => (
              <button
                key={c.index}
                onClick={() => onCite(c)}
                title={t('Open the meeting at this moment')}
                className="flex items-center gap-1 rounded-full bg-[var(--bg-canvas)]/70 px-2 py-0.5 text-xs text-[var(--gold)] ring-1 ring-[var(--gold-ring)] transition-colors hover:bg-[var(--gold-soft)]"
              >
                <Icon name="transcript" size={12} />
                <span className="font-medium">[{c.index}]</span>
                {showMeetingLabel && (
                  <span className="max-w-[160px] truncate">{meetingTitle(c.meeting_id)}</span>
                )}
              </button>
            ))}
          </div>
        )}
      </div>
    </motion.div>
  );
}

export function RetrievalExplanation({ diagnostics }: { diagnostics: RetrievalDiagnostics }) {
  const t = useT();
  let explanation: string;
  if (diagnostics.reason === 'no_index') {
    explanation = t('There are no searchable transcripts in this scope yet.');
  } else if (diagnostics.reason === 'index_incomplete') {
    explanation = t('The archive index is incomplete: {indexed} of {total} meetings are ready.')
      .replace('{indexed}', String(diagnostics.indexed_meetings))
      .replace('{total}', String(diagnostics.indexable_meetings));
  } else if (diagnostics.reason === 'answer_ungrounded') {
    explanation = t('Relevant fragments were found, but the generated answer could not be verified against source links.');
  } else if (diagnostics.reason === 'answer_not_found') {
    explanation = t('Relevant fragments were checked, but they do not contain an answer to this question.');
  } else {
    explanation = t('Memento checked close spellings and related fragments, but did not find enough evidence for a grounded answer.');
  }

  return (
    <div className="mt-2 rounded-lg bg-[var(--bg-canvas)]/60 px-3 py-2 text-xs leading-relaxed text-[var(--fg3)]">
      <p>{explanation}</p>
      {!diagnostics.semantic_available && diagnostics.indexable_meetings > 0 && (
        <p className="mt-1">{t('Semantic search was unavailable; keyword and typo-tolerant search were used.')}</p>
      )}
      {diagnostics.query_rewritten && (
        <p className="mt-1">{t('Common ASR spellings and transliterated product names were included automatically.')}</p>
      )}
    </div>
  );
}

export function TypingIndicator() {
  return (
    <div className="flex justify-start">
      <div className="flex items-center gap-1 rounded-2xl bg-[var(--bg-elevated)] px-4 py-3">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="h-2 w-2 animate-bounce rounded-full bg-[var(--fg3)]"
            style={{ animationDelay: `${i * 0.15}s` }}
          />
        ))}
      </div>
    </div>
  );
}
