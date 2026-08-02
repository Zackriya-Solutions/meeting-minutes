"use client";

import { motion } from 'framer-motion';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';
import { avatarGradients } from '@/vendor/deslop/primitives/tokens.js';
import { Bubble, BubbleContent } from '@/components/ui/bubble';
import { Message, MessageAvatar, MessageContent, MessageHeader } from '@/components/ui/message';
import { ChatMarkdown } from './ChatMarkdown';
import type { ChatMessage, Citation, RetrievalDiagnostics } from '@/hooks/useMeetingChat';

/**
 * Chat primitives shared by the archive/collection chat page and the embedded
 * meeting conversation. Uses the official shadcn Message composition: avatar,
 * sender header, and a visible Bubble surface for both sides. The citation click
 * handler is injected — the archive page routes to `/meeting-details`, the meeting
 * screen expands the transcript pin and scrolls to the cited segment in place.
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
  showMeetingLabel?: boolean;
}) {
  const t = useT();
  const isUser = msg.role === 'user';
  const notFound = msg.role === 'assistant' && msg.found === false && !msg.error;
  const senderName = isUser ? t('You') : 'Memento';
  const avatarGradient = avatarGradients[isUser ? 0 : 2];
  const avatarInitials = isUser ? senderName.slice(0, 1).toUpperCase() : 'M';

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.18 }}
      className="w-full"
    >
      <Message align={isUser ? 'end' : 'start'}>
        <MessageAvatar
          aria-label={senderName}
          title={senderName}
          className="h-8 w-8 text-xs font-bold text-white"
          style={{
            background: `linear-gradient(180deg, ${avatarGradient.top} 0%, ${avatarGradient.bottom} 100%)`,
          }}
        >
          <span aria-hidden="true">{avatarInitials}</span>
        </MessageAvatar>
        <MessageContent>
          <Bubble
            align={isUser ? 'end' : 'start'}
            variant={msg.error ? 'destructive' : isUser ? 'default' : 'muted'}
          >
            <BubbleContent
              className={cn(
                !msg.error && '!bg-[var(--primary-5)] !text-[var(--deslop-primary)]',
              )}
            >
              <div className={cn('flex flex-col gap-1', isUser ? 'items-end' : 'items-start')}>
                <span className="text-xs font-medium text-[var(--deslop-primary-60)]">
                  {senderName}
                </span>

                <div className={cn('w-full', isUser && 'whitespace-pre-wrap')}>
                  {notFound && (
                    <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                      <Icon name="search" size={14} />
                      {t('Not found in your meetings')}
                    </div>
                  )}

                  {isUser || msg.error
                    ? <div className="whitespace-pre-wrap">{msg.content}</div>
                    : <ChatMarkdown content={msg.content} />}

                  {notFound && msg.diagnostics && <RetrievalExplanation diagnostics={msg.diagnostics} />}

                  {msg.warning && (
                    <div className="mt-2 flex items-center gap-1.5 text-xs text-primary">
                      <Icon name="alert" size={14} />
                      {msg.warning}
                    </div>
                  )}

                  {!!msg.citations?.length && (
                    <div className="mt-2.5 flex flex-wrap gap-1.5">
                      {msg.citations.map((c) => (
                        <button
                          key={c.index}
                          onClick={() => onCite(c)}
                          title={t('Open the meeting at this moment')}
                          className="mm-numeric inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-[11px] font-semibold text-muted-foreground transition-colors hover:text-foreground"
                        >
                          <span>[{c.index}]</span>
                          {showMeetingLabel && <span className="max-w-[160px] truncate font-normal">{meetingTitle(c.meeting_id)}</span>}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </BubbleContent>
          </Bubble>
        </MessageContent>
      </Message>
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
    <div className="mt-2 rounded-lg bg-muted px-3 py-2 text-xs leading-relaxed text-muted-foreground">
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
  const t = useT();
  const avatarGradient = avatarGradients[2];

  return (
    <Message align="start" role="status" aria-label={t('Assistant is typing')}>
      <MessageAvatar
        aria-label="Memento"
        title="Memento"
        className="h-8 w-8 text-xs font-bold text-white"
        style={{
          background: `linear-gradient(180deg, ${avatarGradient.top} 0%, ${avatarGradient.bottom} 100%)`,
        }}
      >
        <span aria-hidden="true">M</span>
      </MessageAvatar>
      <MessageContent>
        <MessageHeader>Memento</MessageHeader>
        <Bubble variant="muted">
          <BubbleContent className="flex items-center gap-1">
            {[0, 1, 2].map((i) => (
              <span
                key={i}
                className="h-2 w-2 animate-bounce rounded-full bg-muted-foreground"
                style={{ animationDelay: `${i * 0.15}s` }}
              />
            ))}
          </BubbleContent>
        </Bubble>
      </MessageContent>
    </Message>
  );
}
