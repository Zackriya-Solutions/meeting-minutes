'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { motion } from 'framer-motion';
import {
  ArrowLeft,
  MessageSquare,
  Send,
  Plus,
  Loader2,
  FileText,
  AlertTriangle,
  SearchX,
} from 'lucide-react';
import { cn } from '@/lib/utils';

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
    <div className="flex h-screen flex-col pr-8 pt-4">
      {/* Header */}
      <div className="flex items-center gap-3 border-b border-gray-100 pb-4">
        <button
          onClick={() => router.push('/')}
          className="rounded-lg p-2 text-gray-500 transition-colors hover:bg-gray-100"
          aria-label="Назад"
        >
          <ArrowLeft className="h-5 w-5" />
        </button>
        <MessageSquare className="h-6 w-6 text-blue-600" />
        <h1 className="text-lg font-semibold text-gray-900">Чат с архивом</h1>

        <div className="ml-auto flex items-center gap-2">
          {/* Scope selector */}
          <select
            value={scopeKind}
            onChange={(e) => changeScope(e.target.value as ScopeKind)}
            className="rounded-lg border border-gray-200 bg-white px-3 py-1.5 text-sm text-gray-700 focus:border-blue-400 focus:outline-none"
          >
            <option value="archive">Весь архив</option>
            <option value="collection">Коллекция</option>
            <option value="meeting">Встреча</option>
          </select>

          {scopeKind === 'collection' && (
            <select
              value={collectionId ?? ''}
              onChange={(e) => setCollectionId(e.target.value ? Number(e.target.value) : null)}
              className="max-w-[200px] rounded-lg border border-gray-200 bg-white px-3 py-1.5 text-sm text-gray-700 focus:border-blue-400 focus:outline-none"
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
              className="max-w-[220px] rounded-lg border border-gray-200 bg-white px-3 py-1.5 text-sm text-gray-700 focus:border-blue-400 focus:outline-none"
            >
              <option value="">Выберите встречу…</option>
              {meetings.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.title}
                </option>
              ))}
            </select>
          )}

          <button
            onClick={startNewChat}
            className="flex items-center gap-1.5 rounded-lg border border-gray-200 px-3 py-1.5 text-sm text-gray-700 transition-colors hover:bg-gray-100"
          >
            <Plus className="h-4 w-4" />
            Новый чат
          </button>
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
      <div className="border-t border-gray-100 py-4">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder="Спросите о ваших встречах…"
            className="max-h-40 min-h-[44px] flex-1 resize-none rounded-xl border border-gray-200 px-4 py-2.5 text-sm text-gray-800 focus:border-blue-400 focus:outline-none"
          />
          <button
            onClick={() => send(input)}
            disabled={sending || !input.trim()}
            className={cn(
              'flex h-11 w-11 items-center justify-center rounded-xl text-white transition-colors',
              sending || !input.trim() ? 'cursor-not-allowed bg-gray-300' : 'bg-blue-600 hover:bg-blue-700',
            )}
            aria-label="Отправить"
          >
            {sending ? <Loader2 className="h-5 w-5 animate-spin" /> : <Send className="h-5 w-5" />}
          </button>
        </div>
        <p className="mx-auto mt-2 max-w-3xl text-center text-xs text-gray-400">
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
            ? 'bg-blue-600 text-white'
            : msg.error
              ? 'border border-red-200 bg-red-50 text-red-700'
              : notFound
                ? 'border border-gray-200 bg-gray-50 text-gray-500'
                : 'bg-gray-100 text-gray-800',
        )}
      >
        {notFound && (
          <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-gray-500">
            <SearchX className="h-3.5 w-3.5" />
            Не найдено в ваших встречах
          </div>
        )}
        <div className="whitespace-pre-wrap leading-relaxed">{msg.content}</div>

        {msg.warning && (
          <div className="mt-2 flex items-center gap-1.5 text-xs text-amber-600">
            <AlertTriangle className="h-3.5 w-3.5" />
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
                className="flex items-center gap-1 rounded-full bg-white/70 px-2 py-0.5 text-xs text-blue-700 ring-1 ring-blue-200 transition-colors hover:bg-blue-50"
              >
                <FileText className="h-3 w-3" />
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
      <div className="flex items-center gap-1 rounded-2xl bg-gray-100 px-4 py-3">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="h-2 w-2 animate-bounce rounded-full bg-gray-400"
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
      <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50">
        <MessageSquare className="h-7 w-7 text-blue-600" />
      </div>
      <h2 className="text-xl font-semibold text-gray-900">Спросите свой архив встреч</h2>
      <p className="mt-2 text-sm text-gray-500">
        Задавайте вопросы на естественном языке — ответы приходят со ссылками на конкретные моменты записей.
      </p>
      <div className="mt-8 flex w-full flex-col gap-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            disabled={disabled}
            onClick={() => onPick(s)}
            className="rounded-xl border border-gray-200 px-4 py-3 text-left text-sm text-gray-700 transition-colors hover:border-blue-300 hover:bg-blue-50 disabled:opacity-50"
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}
