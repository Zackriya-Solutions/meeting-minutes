'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { Loader2 } from '@/components/deslop-icons';
import { cn } from '@/lib/utils';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';
import { KnowledgeReadinessCard } from '@/components/KnowledgeReadinessCard';
import { useMeetingChat, type Citation, type ScopeKind } from '@/hooks/useMeetingChat';
import { MessageBubble, TypingIndicator } from '@/components/chat/MessageBubble';
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from '@/components/ui/message-scroller';
import { ChatDrawerShell } from './chat-drawer-shell';

interface MeetingRef {
  id: string;
  title: string;
}

const SUGGESTIONS = [
  'What did we decide about the budget over the last month?',
  'What open tasks does the team have?',
  'What did we agree with the client at the last meeting?',
];

export default function ChatPage() {
  const router = useRouter();
  const t = useT();

  const [scopeInitialized, setScopeInitialized] = useState(false);
  const [scopeKind, setScopeKind] = useState<ScopeKind>('archive');
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [meetings, setMeetings] = useState<MeetingRef[]>([]);

  const pendingQuestion = useRef<string | null>(null);
  const submittedPendingQuestion = useRef(false);

  const chat = useMeetingChat({
    scope: scopeKind,
    collectionId,
    meetingId,
    enabled: scopeInitialized,
  });
  const { messages, input, setInput, sending, loadingHistory, send, onKeyDown, inputRef } = chat;

  // Load scope options.
  useEffect(() => {
    invoke<MeetingRef[]>('api_get_meetings')
      .then((m) => setMeetings(Array.isArray(m) ? m : []))
      .catch(() => setMeetings([]));
  }, []);

  // Collection and meeting workspaces deep-link here with the scope already selected.
  // Read the URL in an effect so the page remains compatible with static export.
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const scope = params.get('scope');
    pendingQuestion.current = params.get('question')?.trim() || null;
    if (params.has('question')) {
      params.delete('question');
      const nextSearch = params.toString();
      window.history.replaceState(
        window.history.state,
        '',
        `${window.location.pathname}${nextSearch ? `?${nextSearch}` : ''}`,
      );
    }
    submittedPendingQuestion.current = false;
    if (scope === 'collection') {
      const requestedCollectionId = Number(params.get('collectionId'));
      if (Number.isInteger(requestedCollectionId) && requestedCollectionId > 0) {
        setScopeKind('collection');
        setCollectionId(requestedCollectionId);
      } else {
        setScopeKind('archive');
      }
    } else if (scope === 'meeting') {
      const requestedMeetingId = params.get('meetingId')?.trim();
      if (requestedMeetingId) {
        setScopeKind('meeting');
        setMeetingId(requestedMeetingId);
      } else {
        setScopeKind('archive');
      }
    } else {
      setScopeKind('archive');
    }
    setScopeInitialized(true);
  }, []);

  const openCitation = useCallback(
    (c: Citation) => {
      const seconds = Math.floor(c.start_ms / 1000);
      router.push(`/meeting-details?id=${encodeURIComponent(c.meeting_id)}&t=${seconds}`);
    },
    [router],
  );

  useEffect(() => {
    const question = pendingQuestion.current;
    if (!scopeInitialized || loadingHistory || sending || submittedPendingQuestion.current || !question) return;

    submittedPendingQuestion.current = true;
    pendingQuestion.current = null;
    void send(question);
  }, [loadingHistory, scopeInitialized, send, sending]);

  const meetingTitle = (id: string) => meetings.find((m) => m.id === id)?.title ?? id.slice(0, 8);
  return (
    <ChatDrawerShell>
    <div className="mm-page drawer-elevation-surface !h-full">
      {/* Header */}
      <div className="mm-page-header">
        <h1 className="memento-serif-title truncate text-2xl leading-tight text-foreground">
          {scopeKind === 'collection' ? t('Chat with collection') : t('Chat with archive')}
        </h1>
      </div>

      {/* Messages */}
      <MessageScrollerProvider
        key={`${scopeKind}-${collectionId ?? 'all'}-${meetingId ?? 'all'}-${loadingHistory ? 'loading' : 'ready'}`}
        autoScroll
        defaultScrollPosition="last-anchor"
        scrollPreviousItemPeek={48}
      >
        <MessageScroller className="min-h-0 flex-1">
          <MessageScrollerViewport className="px-6 py-6">
            <MessageScrollerContent className="mx-auto w-full max-w-3xl gap-5">
              {loadingHistory ? (
                <MessageScrollerItem className="flex min-h-[240px] items-center justify-center text-muted-foreground">
                  <Loader2 className="h-5 w-5 animate-spin" />
                </MessageScrollerItem>
              ) : messages.length === 0 ? (
                <>
                  <MessageScrollerItem messageId="knowledge-readiness">
                    <KnowledgeReadinessCard mode="chat" />
                  </MessageScrollerItem>
                  <MessageScrollerItem messageId="knowledge-empty-state">
                    <EmptyState onPick={(s) => send(s)} disabled={sending} />
                  </MessageScrollerItem>
                </>
              ) : (
                <>
                  {messages.map((msg, i) => (
                    <MessageScrollerItem
                      key={`${msg.role}-${i}`}
                      messageId={`knowledge-chat-${i}`}
                      scrollAnchor={msg.role === 'user'}
                    >
                      <MessageBubble msg={msg} meetingTitle={meetingTitle} onCite={openCitation} />
                    </MessageScrollerItem>
                  ))}
                  {sending && (
                    <MessageScrollerItem messageId="knowledge-chat-typing">
                      <TypingIndicator />
                    </MessageScrollerItem>
                  )}
                </>
              )}
            </MessageScrollerContent>
          </MessageScrollerViewport>
          <MessageScrollerButton />
        </MessageScroller>
      </MessageScrollerProvider>

      {/* Composer */}
      <div className="border-t border-border py-4">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <textarea
            ref={inputRef}
            value={input}
            disabled={loadingHistory}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder={t('Ask about your meetings…')}
            className="mm-field max-h-40 min-h-[48px] flex-1 resize-none py-3 text-sm outline-none"
          />
          <button
            onClick={() => send(input)}
            disabled={loadingHistory || sending || !input.trim()}
            className={cn(
              'flex h-11 w-11 items-center justify-center rounded-xl text-primary-foreground transition-colors',
              loadingHistory || sending || !input.trim() ? 'cursor-not-allowed bg-muted' : 'bg-primary hover:bg-primary/90',
            )}
            aria-label={t('Send')}
          >
            {sending ? <Loader2 className="h-5 w-5 animate-spin" /> : <Icon name="send" />}
          </button>
        </div>
        <p className="mx-auto mt-2 max-w-3xl text-center text-xs text-muted-foreground">
          {t('Answers are drawn only from your recordings, with links to the sources.')}
        </p>
      </div>
    </div>
    </ChatDrawerShell>
  );
}

function EmptyState({ onPick, disabled }: { onPick: (s: string) => void; disabled: boolean }) {
  const t = useT();
  return (
    <div className="mx-auto flex max-w-2xl flex-col items-center pt-16 text-center">
      <div className="mm-empty-icon mb-4">
        <Icon name="library" size={28} />
      </div>
      <h2 className="text-xl font-semibold text-foreground">{t('Ask your meeting archive')}</h2>
      <p className="mt-2 text-sm text-muted-foreground">
        {t('Ask questions in natural language — answers come with links to specific moments in your recordings.')}
      </p>
      <div className="mt-8 flex w-full flex-col gap-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            disabled={disabled}
            onClick={() => onPick(t(s))}
            className="rounded-xl border border-border px-4 py-3 text-left text-sm text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/10 disabled:opacity-50"
          >
            {t(s)}
          </button>
        ))}
      </div>
    </div>
  );
}
