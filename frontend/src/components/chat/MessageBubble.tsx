"use client";

import { useState } from 'react';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';
import { avatarGradients } from '@/vendor/deslop/primitives/tokens.js';
import { Bubble, BubbleContent } from '@/components/ui/bubble';
import { Badge } from '@/components/ui/fluid-badge';
import { Message, MessageAvatar, MessageContent } from '@/components/ui/message';
import { ChatMarkdown } from './ChatMarkdown';
import type { ChatMessage, Citation } from '@/hooks/useMeetingChat';
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
  const senderName = isUser ? t('You') : 'Memento';
  const avatarGradient = avatarGradients[isUser ? 0 : 2];
  const avatarInitials = isUser ? senderName.slice(0, 1).toUpperCase() : 'M';
  const [showFormattedAnswer, setShowFormattedAnswer] = useState(!msg.animate);

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
              <div className="flex flex-col items-start gap-0.5 text-left">
                <span className="text-xs font-medium text-[var(--deslop-primary-60)]">
                  {senderName}
                </span>

                <div className={cn('w-full text-left', isUser && 'whitespace-pre-wrap')}>
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
        <Bubble variant="muted">
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
