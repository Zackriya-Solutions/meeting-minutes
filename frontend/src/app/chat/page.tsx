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

type ScopeKind = 'archive' | 'collection' | 'meeting';

interface MeetingRef {
  id: string;
  title: string;
}
interface CollectionRef {
  id: number;
  name: string;
}

const SUGGESTIONS = [
  'Что мы решили по бюджету за последний месяц?',
  'Какие открытые задачи у команды?',
  'О чём договорились с клиентом на последней встрече?',
];

export default function ChatPage() {
  const router = useRouter();

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);

  const [scopeKind, setScopeKind] = useState<ScopeKind>('archive');
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const [meetingId, setMeetingId] = useState<string | null>(null);

  const [meetings, setMeetings] = useState<MeetingRef[]>([]);
  const [collections, setCollections] = useState<CollectionRef[]>([]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Load scope options.
  useEffect(() => {
    invoke<MeetingRef[]>('api_get_meetings')
      .then((m) => setMeetings(Array.isArray(m) ? m : []))
      .catch(() => setMeetings([]));
    invoke<CollectionRef[]>('list_collections')
      .then((c) => setCollections(Array.isArray(c) ? c : []))
      .catch(() => setCollections([]));
  }, []);

  // Auto-scroll to the latest message.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [messages, sending]);

  const startNewChat = useCallback(() => {
    setMessages([]);
    setSessionId(null);
    inputRef.current?.focus();
  }, []);

  // Changing scope starts a fresh session (chat history is per-scope).
  const changeScope = (kind: ScopeKind) => {
    setScopeKind(kind);
    setSessionId(null);
    setMessages([]);
  };

  const openCitation = (c: Citation) => {
    const seconds = Math.floor(c.start_ms / 1000);
    router.push(`/meeting-details?id=${encodeURIComponent(c.meeting_id)}&t=${seconds}`);
  };

  const send = useCallback(
    async (text: string) => {
      const query = text.trim();
      if (!query || sending) return;

      if (scopeKind === 'collection' && collectionId == null) {
        setMessages((m) => [...m, { role: 'assistant', content: 'Выберите коллекцию для поиска.', error: true }]);
        return;
      }
      if (scopeKind === 'meeting' && !meetingId) {
        setMessages((m) => [...m, { role: 'assistant', content: 'Выберите встречу для поиска.', error: true }]);
        return;
      }

      setInput('');
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
                : 'Не удалось получить ответ. Проверьте, что настроен провайдер LLM (GigaChat или DeepSeek) в настройках.',
            error: true,
          },
        ]);
      } finally {
        setSending(false);
      }
    },
    [sending, scopeKind, collectionId, meetingId, sessionId],
  );

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send(input);
    }
  };

  const meetingTitle = (id: string) => meetings.find((m) => m.id === id)?.title ?? id.slice(0, 8);

  return (
    <div className="mm-page">
      {/* Header */}
      <div className="mm-page-header">
        <button
          onClick={() => router.push('/')}
          className="mm-icon-button mm-hover"
          aria-label="Назад"
        >
          <Icon name="back" />
        </button>
        <Icon name="library" size={24} className="text-[var(--gold)]" />
        <h1 className="mm-page-title">База знаний</h1>

        <div className="ml-auto flex items-center gap-2">
          {/* Scope selector */}
          <select
            value={scopeKind}
            onChange={(e) => changeScope(e.target.value as ScopeKind)}
            className="mm-select"
          >
            <option value="archive">Весь архив</option>
            <option value="collection">Коллекция</option>
            <option value="meeting">Встреча</option>
          </select>

          {scopeKind === 'collection' && (
            <select
              value={collectionId ?? ''}
              onChange={(e) => setCollectionId(e.target.value ? Number(e.target.value) : null)}
              className="mm-select max-w-[200px]"
            >
              <option value="">Выберите коллекцию…</option>
              {collections.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
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
              <option value="">Выберите встречу…</option>
              {meetings.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.title}
                </option>
              ))}
            </select>
          )}

          <Button
            onClick={startNewChat}
            variant="secondary"
            size="sm"
            icon={<Icon name="plus" size={16} />}
          >
            Новый чат
          </Button>
        </div>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto py-6">
        {messages.length === 0 ? (
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
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder="Спросите о ваших встречах…"
            className="mm-field max-h-40 min-h-[48px] flex-1 resize-none py-3 text-sm outline-none"
          />
          <button
            onClick={() => send(input)}
            disabled={sending || !input.trim()}
            className={cn(
              'flex h-11 w-11 items-center justify-center rounded-xl text-[var(--fg-inverse)] transition-colors',
              sending || !input.trim() ? 'cursor-not-allowed bg-[var(--bg-elevated)]' : 'bg-[var(--gold)] hover:bg-[var(--gold)]',
            )}
            aria-label="Отправить"
          >
            {sending ? <Loader2 className="h-5 w-5 animate-spin" /> : <Icon name="send" />}
          </button>
        </div>
        <p className="mx-auto mt-2 max-w-3xl text-center text-xs text-[var(--fg3)]">
          Ответы формируются только из ваших записей, со ссылками на источники.
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
            Не найдено в ваших встречах
          </div>
        )}
        <div className="whitespace-pre-wrap leading-relaxed">{msg.content}</div>

        {msg.warning && (
          <div className="mt-2 flex items-center gap-1.5 text-xs text-amber-600">
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
                title="Открыть встречу на этом месте"
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
  return (
    <div className="mx-auto flex max-w-2xl flex-col items-center pt-16 text-center">
      <div className="mm-empty-icon mb-4">
        <Icon name="library" size={28} />
      </div>
      <h2 className="text-xl font-semibold text-[var(--fg1)]">Спросите свой архив встреч</h2>
      <p className="mt-2 text-sm text-[var(--fg2)]">
        Задавайте вопросы на естественном языке — ответы приходят со ссылками на конкретные моменты записей.
      </p>
      <div className="mt-8 flex w-full flex-col gap-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            disabled={disabled}
            onClick={() => onPick(s)}
            className="rounded-xl border border-[var(--border-subtle)] px-4 py-3 text-left text-sm text-[var(--fg2)] transition-colors hover:border-[var(--gold-border)] hover:bg-[var(--gold-soft)] disabled:opacity-50"
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}
