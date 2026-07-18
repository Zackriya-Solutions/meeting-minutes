'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { motion } from 'framer-motion';
import {
  Loader2,
} from '@/components/memento/LucideCompat';
import { cn } from '@/lib/utils';
import { Icon } from '@/components/memento/Icon';
import { Button } from '@/components/memento/Button';
import { getCollectionDisplayText } from '@/lib/collectionDisplay';
import { useT } from '@/lib/i18n';
import { KnowledgeReadinessCard } from '@/components/KnowledgeReadinessCard';

// Mirrors the Rust `Citation` (search::rag).
interface Citation {
  index: number;
  chunk_id: number;
  meeting_id: string;
  start_ms: number;
}

// Mirrors the Rust `RagAskResponse` (search::commands).
interface RagAskResponse {
  session_id: number;
  answer: string;
  citations: Citation[];
  found: boolean;
  warning: string | null;
}

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  citations?: Citation[];
  found?: boolean;
  warning?: string | null;
  error?: boolean;
}

interface RagSessionResponse {
  session_id: number;
  messages: ChatMessage[];
}

type ScopeKind = 'archive' | 'collection' | 'meeting';

interface MeetingRef {
  id: string;
  title: string;
}
interface CollectionRef {
  id: number;
  name: string;
  kind: 'manual' | 'series';
  system_key?: string | null;
}

const SUGGESTIONS = [
  'What did we decide about the budget over the last month?',
  'What open tasks does the team have?',
  'What did we agree with the client at the last meeting?',
];

export default function ChatPage() {
  const router = useRouter();
  const t = useT();

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(true);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [scopeInitialized, setScopeInitialized] = useState(false);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  const [scopeKind, setScopeKind] = useState<ScopeKind>('archive');
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [returnCollectionId, setReturnCollectionId] = useState<number | null>(null);

  const [meetings, setMeetings] = useState<MeetingRef[]>([]);
  const [collections, setCollections] = useState<CollectionRef[]>([]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const draftBeforeHistory = useRef('');
  const pendingQuestion = useRef<string | null>(null);
  const submittedPendingQuestion = useRef(false);

  // Load scope options.
  useEffect(() => {
    invoke<MeetingRef[]>('api_get_meetings')
      .then((m) => setMeetings(Array.isArray(m) ? m : []))
      .catch(() => setMeetings([]));
    invoke<CollectionRef[]>('list_collections')
      .then((c) => setCollections(Array.isArray(c) ? c : []))
      .catch(() => setCollections([]));
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
        setReturnCollectionId(requestedCollectionId);
      } else {
        setScopeKind('archive');
        setReturnCollectionId(null);
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
      setReturnCollectionId(null);
    }
    setSessionId(null);
    setMessages([]);
    setScopeInitialized(true);
  }, []);

  // Conversations are persisted by the Rust core. Restore the latest session for the
  // selected archive/collection/meeting whenever this screen is mounted or scope changes.
  useEffect(() => {
    if (!scopeInitialized) return;
    if (scopeKind === 'collection' && collectionId == null) {
      setLoadingHistory(false);
      return;
    }
    if (scopeKind === 'meeting' && !meetingId) {
      setLoadingHistory(false);
      return;
    }

    let active = true;
    setLoadingHistory(true);
    setMessages([]);
    setSessionId(null);
    setHistoryIndex(null);
    invoke<RagSessionResponse | null>('rag_get_latest_session', {
      input: {
        scope: scopeKind,
        collection_id: scopeKind === 'collection' ? collectionId : null,
        meeting_id: scopeKind === 'meeting' ? meetingId : null,
      },
    })
      .then((session) => {
        if (!active || !session) return;
        setSessionId(session.session_id);
        setMessages(Array.isArray(session.messages) ? session.messages : []);
      })
      .catch((error) => {
        console.error('Failed to restore chat session:', error);
      })
      .finally(() => {
        if (active) setLoadingHistory(false);
      });

    return () => {
      active = false;
    };
  }, [scopeInitialized, scopeKind, collectionId, meetingId]);

  // Auto-scroll to the latest message.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [messages, sending]);

  const startNewChat = useCallback(() => {
    setMessages([]);
    setSessionId(null);
    setInput('');
    setHistoryIndex(null);
    draftBeforeHistory.current = '';
    inputRef.current?.focus();
  }, []);

  // Changing scope restores the latest session for that scope.
  const changeScope = (kind: ScopeKind) => {
    setScopeKind(kind);
    if (kind !== 'collection') setCollectionId(null);
    if (kind !== 'meeting') setMeetingId(null);
    setSessionId(null);
    setMessages([]);
    setHistoryIndex(null);
  };

  const openCitation = (c: Citation) => {
    const seconds = Math.floor(c.start_ms / 1000);
    router.push(`/meeting-details?id=${encodeURIComponent(c.meeting_id)}&t=${seconds}`);
  };

  const send = useCallback(
    async (text: string) => {
      const query = text.trim();
      if (!query || sending || loadingHistory) return;

      if (scopeKind === 'collection' && collectionId == null) {
        setMessages((m) => [...m, { role: 'assistant', content: t('Select a collection to search.'), error: true }]);
        return;
      }
      if (scopeKind === 'meeting' && !meetingId) {
        setMessages((m) => [...m, { role: 'assistant', content: t('Select a meeting to search.'), error: true }]);
        return;
      }

      setInput('');
      setHistoryIndex(null);
      draftBeforeHistory.current = '';
      setMessages((m) => [...m, { role: 'user', content: query }]);
      setSending(true);

      try {
        const res = await invoke<RagAskResponse>('rag_ask', {
          input: {
            query,
            scope: scopeKind,
            collection_id: scopeKind === 'collection' ? collectionId : null,
            meeting_id: scopeKind === 'meeting' ? meetingId : null,
            session_id: sessionId,
          },
        });
        setSessionId(res.session_id);
        setMessages((m) => [
          ...m,
          {
            role: 'assistant',
            content: res.answer,
            citations: res.citations ?? [],
            found: res.found,
            warning: res.warning,
          },
        ]);
      } catch (e) {
        setMessages((m) => [
          ...m,
          {
            role: 'assistant',
            content:
              typeof e === 'string'
                ? e
                : t('Failed to get an answer. Make sure an LLM provider (GigaChat or DeepSeek) is configured in settings.'),
            error: true,
          },
        ]);
      } finally {
        setSending(false);
      }
    },
    [sending, loadingHistory, scopeKind, collectionId, meetingId, sessionId, t],
  );

  useEffect(() => {
    const question = pendingQuestion.current;
    if (
      !scopeInitialized
      || loadingHistory
      || sending
      || submittedPendingQuestion.current
      || !question
    ) return;

    submittedPendingQuestion.current = true;
    pendingQuestion.current = null;
    void send(question);
  }, [loadingHistory, scopeInitialized, send, sending]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send(input);
      return;
    }

    const queryHistory = messages
      .filter((message) => message.role === 'user')
      .map((message) => message.content);
    if (
      e.key === 'ArrowUp'
      && !e.altKey
      && !e.ctrlKey
      && !e.metaKey
      && queryHistory.length > 0
      && (historyIndex != null || input.length === 0 || e.currentTarget.selectionStart === 0)
    ) {
      e.preventDefault();
      const nextIndex = historyIndex == null
        ? queryHistory.length - 1
        : Math.max(0, historyIndex - 1);
      if (historyIndex == null) draftBeforeHistory.current = input;
      setHistoryIndex(nextIndex);
      setInput(queryHistory[nextIndex]);
      return;
    }
    if (
      e.key === 'ArrowDown'
      && !e.altKey
      && !e.ctrlKey
      && !e.metaKey
      && historyIndex != null
    ) {
      e.preventDefault();
      if (historyIndex < queryHistory.length - 1) {
        const nextIndex = historyIndex + 1;
        setHistoryIndex(nextIndex);
        setInput(queryHistory[nextIndex]);
      } else {
        setHistoryIndex(null);
        setInput(draftBeforeHistory.current);
      }
    }
  };

  const meetingTitle = (id: string) => meetings.find((m) => m.id === id)?.title ?? id.slice(0, 8);
  const selectedCollection = collections.find((collection) => collection.id === collectionId);
  const selectedCollectionName = selectedCollection
    ? getCollectionDisplayText(selectedCollection, t).name
    : null;
  const backToContext = () => {
    if (returnCollectionId != null) {
      router.push(`/collections?collectionId=${returnCollectionId}`);
      return;
    }
    router.push('/');
  };

  return (
    <div className="mm-page">
      {/* Header */}
      <div className="mm-page-header">
        <button
          onClick={backToContext}
          className="mm-icon-button mm-hover"
          aria-label={returnCollectionId != null ? t('Back to collection') : t('Back')}
          title={returnCollectionId != null ? t('Back to collection') : t('Back')}
        >
          <Icon name="back" />
        </button>
        <Icon name="library" size={24} className="text-[var(--gold)]" />
        <div className="min-w-0">
          <h1 className="mm-page-title">
            {scopeKind === 'collection' ? t('Chat with collection') : t('Chat with archive')}
          </h1>
          {scopeKind === 'collection' && selectedCollectionName && (
            <p className="mt-0.5 truncate text-xs text-[var(--fg3)]">{selectedCollectionName}</p>
          )}
        </div>

        <div className="ml-auto flex items-center gap-2">
          {/* Scope selector */}
          <select
            value={scopeKind}
            onChange={(e) => changeScope(e.target.value as ScopeKind)}
            className="mm-select"
          >
            <option value="archive">{t('Entire archive')}</option>
            <option value="collection">{t('Collection')}</option>
            <option value="meeting">{t('Meeting')}</option>
          </select>

          {scopeKind === 'collection' && (
            <select
              value={collectionId ?? ''}
              onChange={(e) => setCollectionId(e.target.value ? Number(e.target.value) : null)}
              className="mm-select max-w-[200px]"
            >
              <option value="">{t('Select a collection…')}</option>
              {collections.map((c) => (
                <option key={c.id} value={c.id}>
                  {getCollectionDisplayText(c, t).name}
                </option>
              ))}
            </select>
          )}

          {scopeKind === 'meeting' && (
            <select
              value={meetingId ?? ''}
              onChange={(e) => setMeetingId(e.target.value || null)}
              className="mm-select max-w-[220px]"
            >
              <option value="">{t('Select a meeting…')}</option>
              {meetings.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.title}
                </option>
              ))}
            </select>
          )}

          <Button
            onClick={startNewChat}
            disabled={loadingHistory}
            variant="secondary"
            size="sm"
            icon={<Icon name="plus" size={16} />}
          >
            {t('New chat')}
          </Button>
        </div>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto py-6">
        {!loadingHistory && messages.length === 0 && <KnowledgeReadinessCard mode="chat" />}
        {loadingHistory ? (
          <div className="flex h-full items-center justify-center text-[var(--fg3)]">
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        ) : messages.length === 0 ? (
          <EmptyState onPick={(s) => send(s)} disabled={sending} />
        ) : (
          <div className="mx-auto flex max-w-3xl flex-col gap-5">
            {messages.map((msg, i) => (
              <MessageBubble key={i} msg={msg} meetingTitle={meetingTitle} onCite={openCitation} />
            ))}
            {sending && <TypingIndicator />}
          </div>
        )}
      </div>

      {/* Composer */}
      <div className="border-t border-[var(--border-subtle)] py-4">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <textarea
            ref={inputRef}
            value={input}
            disabled={loadingHistory}
            onChange={(e) => {
              setInput(e.target.value);
              setHistoryIndex(null);
              draftBeforeHistory.current = e.target.value;
            }}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder={t('Ask about your meetings…')}
            className="mm-field max-h-40 min-h-[48px] flex-1 resize-none py-3 text-sm outline-none"
          />
          <button
            onClick={() => send(input)}
            disabled={loadingHistory || sending || !input.trim()}
            className={cn(
              'flex h-11 w-11 items-center justify-center rounded-xl text-[var(--fg-inverse)] transition-colors',
              loadingHistory || sending || !input.trim() ? 'cursor-not-allowed bg-[var(--bg-elevated)]' : 'bg-[var(--gold)] hover:bg-[var(--gold-active)]',
            )}
            aria-label={t('Send')}
          >
            {sending ? <Loader2 className="h-5 w-5 animate-spin" /> : <Icon name="send" />}
          </button>
        </div>
        <p className="mx-auto mt-2 max-w-3xl text-center text-xs text-[var(--fg3)]">
          {t('Answers are drawn only from your recordings, with links to the sources.')}
        </p>
      </div>
    </div>
  );
}

function MessageBubble({
  msg,
  meetingTitle,
  onCite,
}: {
  msg: ChatMessage;
  meetingTitle: (id: string) => string;
  onCite: (c: Citation) => void;
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
                <span className="max-w-[160px] truncate">{meetingTitle(c.meeting_id)}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </motion.div>
  );
}

function TypingIndicator() {
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

function EmptyState({ onPick, disabled }: { onPick: (s: string) => void; disabled: boolean }) {
  const t = useT();
  return (
    <div className="mx-auto flex max-w-2xl flex-col items-center pt-16 text-center">
      <div className="mm-empty-icon mb-4">
        <Icon name="library" size={28} />
      </div>
      <h2 className="text-xl font-semibold text-[var(--fg1)]">{t('Ask your meeting archive')}</h2>
      <p className="mt-2 text-sm text-[var(--fg2)]">
        {t('Ask questions in natural language — answers come with links to specific moments in your recordings.')}
      </p>
      <div className="mt-8 flex w-full flex-col gap-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            disabled={disabled}
            onClick={() => onPick(t(s))}
            className="rounded-xl border border-[var(--border-subtle)] px-4 py-3 text-left text-sm text-[var(--fg2)] transition-colors hover:border-[var(--gold-border)] hover:bg-[var(--gold-soft)] disabled:opacity-50"
          >
            {t(s)}
          </button>
        ))}
      </div>
    </div>
  );
}
