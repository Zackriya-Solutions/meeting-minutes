"use client";

import { useState } from 'react';
import { motion } from 'framer-motion';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';
import { Bubble, BubbleContent } from '@/components/ui/bubble';
import { Badge } from '@/components/ui/fluid-badge';
import { Message, MessageAvatar, MessageContent } from '@/components/ui/message';
import { ProceduralSpeakerAvatar } from '@/components/MeetingConversation/ProceduralSpeakerAvatar';
import { ChatMarkdown } from './ChatMarkdown';
import type { ChatMessage, Citation, RetrievalDiagnostics } from '@/hooks/useMeetingChat';
import StreamingText from '@/vendor/deslop/mini-app/StreamingText';

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
  const [showFormattedAnswer, setShowFormattedAnswer] = useState(!msg.animate);

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.18 }}
      className="w-full"
    >
      <Message
        align={isUser ? 'end' : 'start'}
        className="mb-3 gap-0 rounded-lg px-1 py-0.5"
      >
        <MessageAvatar
          aria-label={senderName}
          title={senderName}
          className="h-[60px] w-[60px] min-w-[60px] rounded-none bg-transparent"
        >
          <ProceduralSpeakerAvatar
            speakerId={0}
            preset={isUser ? 'participant' : 'sphere'}
          />
        </MessageAvatar>
        <MessageContent className="gap-0.5">
          <Bubble
            align={isUser ? 'end' : 'start'}
            variant={msg.error ? 'destructive' : isUser ? 'default' : 'muted'}
            className={cn(
              'max-w-[82%]',
              isUser
                ? 'rounded-[16px_16px_4px_16px]'
                : 'rounded-[16px_16px_16px_4px]',
            )}
          >
            <BubbleContent
              className={cn(
                !msg.error && '!bg-[var(--primary-5)] !text-[var(--deslop-primary)]',
              )}
            >
              <div className="flex flex-col items-start gap-0.5 text-left">
                <span className="text-xs font-medium text-[var(--deslop-primary-60)]">
                  {senderName}
                </span>

                <div className={cn('w-full text-left', isUser && 'whitespace-pre-wrap')}>
                  {notFound && (
                    <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                      <Icon name="search" size={14} />
                      {t('Not found in your meetings')}
                    </div>
                  )}

                  {isUser || msg.error ? (
                    <div className="whitespace-pre-wrap">{msg.content}</div>
                  ) : showFormattedAnswer ? (
                    <ChatMarkdown content={msg.content} className="mm-md-answer" />
                  ) : (
                    <div className="whitespace-pre-wrap">
                      <StreamingText
                        mode="word"
                        speed="fast"
                        replayKey={`${msg.role}-${msg.content}`}
                        onComplete={() => setShowFormattedAnswer(true)}
                      >
                        {msg.content}
                      </StreamingText>
                    </div>
                  )}

                  {notFound && msg.diagnostics && <RetrievalExplanation diagnostics={msg.diagnostics} />}

                  {msg.warning && (
                    <div className="mt-2 flex items-center gap-1.5 text-xs text-primary">
                      <Icon name="alert" size={14} />
                      {msg.warning}
                    </div>
                  )}

                  {!!msg.citations?.length && (
                    <div className="mt-2.5 flex w-full flex-wrap items-center justify-start gap-1.5">
                      {msg.citations.map((c, index) => (
                        <motion.button
                          key={c.index}
                          type="button"
                          onClick={() => onCite(c)}
                          title={t('Open the meeting at this moment')}
                          initial={{ opacity: 0, scale: 0.85, filter: 'blur(4px)' }}
                          animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
                          transition={{
                            type: 'spring',
                            stiffness: 420,
                            damping: 30,
                            delay: index * 0.04,
                            filter: { duration: 0.12, delay: index * 0.04 },
                          }}
                          className="min-w-0 text-left outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-20)] focus-visible:ring-offset-1"
                        >
                          <Badge
                            variant="solid"
                            size="sm"
                            color="gray"
                            className="max-w-full cursor-pointer overflow-hidden text-[var(--deslop-primary-60)] transition-colors hover:text-[var(--deslop-primary)] [&>span]:min-w-0 [&>span]:truncate"
                          >
                            <span className="mm-numeric font-semibold">[{c.index}]</span>
                            {showMeetingLabel && (
                              <span className="max-w-[190px] truncate font-normal">
                                {meetingTitle(c.meeting_id)}
                              </span>
                            )}
                          </Badge>
                        </motion.button>
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

/**
 * Why a question came back unanswered. The RAG layer already distinguishes an empty
 * index from a complete one that simply lacks evidence, and `rag_answer_v3` is
 * allowed to infer from indirect signals — so when it still declines, saying which
 * of those happened is the difference between "nothing to search" and "searched,
 * found nothing".
 */
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

  return (
    <Message
      align="start"
      role="status"
      aria-label={t('Assistant is typing')}
      className="mb-3 gap-0 rounded-lg px-1 py-0.5"
    >
      <MessageAvatar
        aria-label="Memento"
        title="Memento"
        className="h-[60px] w-[60px] min-w-[60px] rounded-none bg-transparent"
      >
        <ProceduralSpeakerAvatar speakerId={0} preset="sphere" />
      </MessageAvatar>
      <MessageContent className="gap-0.5">
        <Bubble
          variant="muted"
          className="max-w-[82%] rounded-[16px_16px_16px_4px]"
        >
          <BubbleContent className="flex flex-col items-start gap-1">
            <span className="text-xs font-medium text-[var(--deslop-primary-60)]">
              Memento
            </span>
            <span className="flex h-3 items-center justify-center gap-1" aria-hidden="true">
              {[0, 1, 2].map((i) => (
                <motion.span
                  key={i}
                  className="h-2 w-2 rounded-full bg-muted-foreground"
                  animate={{ opacity: [0.45, 1, 0.45], scale: [0.82, 1, 0.82] }}
                  transition={{
                    duration: 0.9,
                    repeat: Number.POSITIVE_INFINITY,
                    ease: 'easeInOut',
                    delay: i * 0.15,
                  }}
                />
              ))}
            </span>
          </BubbleContent>
        </Bubble>
      </MessageContent>
    </Message>
  );
}
