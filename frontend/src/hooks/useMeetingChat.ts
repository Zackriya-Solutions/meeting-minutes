"use client";

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '@/lib/i18n';

/**
 * Shared meeting/archive/collection chat logic, extracted from `app/chat/page.tsx`
 * so the same RAG conversation can be embedded inside the meeting screen (variant
 * 2a) and the standalone archive/collection chat page without duplicating the
 * send/history/session/keyboard behavior.
 *
 * The hook is scope-driven: callers pass the resolved scope + ids (the URL parsing
 * that the standalone page does stays in the page). No behavior change vs. the
 * original page — the types below mirror the Rust `search::rag` / `search::commands`
 * shapes exactly.
 */

// Mirrors the Rust `Citation` (search::rag).
export interface Citation {
  index: number;
  chunk_id: number;
  meeting_id: string;
  start_ms: number;
}

export interface RetrievalDiagnostics {
  reason:
    | 'ok'
    | 'no_index'
    | 'index_incomplete'
    | 'no_relevant_evidence'
    | 'answer_not_found'
    | 'answer_ungrounded'
    | string;
  indexable_meetings: number;
  indexed_meetings: number;
  best_score: number;
  query_rewritten: boolean;
  semantic_available: boolean;
  lexical_hits: number;
  fuzzy_hits: number;
  semantic_hits: number;
  transcript_fallback_hits: number;
  candidates_considered: number;
}

// Mirrors the Rust `RagAskResponse` (search::commands).
export interface RagAskResponse {
  session_id: number;
  answer: string;
  citations: Citation[];
  found: boolean;
  warning: string | null;
  diagnostics: RetrievalDiagnostics;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  citations?: Citation[];
  found?: boolean;
  warning?: string | null;
  diagnostics?: RetrievalDiagnostics;
  error?: boolean;
  /** Animate only a response that arrived during the current UI session. */
  animate?: boolean;
}

export interface RagSessionResponse {
  session_id: number;
  messages: ChatMessage[];
}

export type ScopeKind = 'archive' | 'collection' | 'meeting';

export interface UseMeetingChatOptions {
  scope: ScopeKind;
  collectionId: number | null;
  meetingId: string | null;
  /**
   * Gate history restoration until the caller has resolved the scope (the
   * standalone page reads the scope from the URL in an effect). Defaults to true
   * for callers that always know their scope up front (e.g. the meeting screen).
   */
  enabled?: boolean;
}

export interface UseMeetingChat {
  messages: ChatMessage[];
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  input: string;
  setInput: (value: string) => void;
  sending: boolean;
  loadingHistory: boolean;
  sessionId: number | null;
  historyIndex: number | null;
  send: (text: string) => Promise<void>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  startNewChat: () => void;
  inputRef: React.RefObject<HTMLTextAreaElement>;
}

export function useMeetingChat({
  scope,
  collectionId,
  meetingId,
  enabled = true,
}: UseMeetingChatOptions): UseMeetingChat {
  const t = useT();

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInputState] = useState('');
  const [sending, setSending] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(true);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const draftBeforeHistory = useRef('');

  const setInput = useCallback((value: string) => {
    setInputState(value);
    setHistoryIndex(null);
    draftBeforeHistory.current = value;
  }, []);

  // Conversations are persisted by the Rust core. Restore the latest session for
  // the selected archive/collection/meeting whenever scope changes.
  useEffect(() => {
    if (!enabled) return;
    if (scope === 'collection' && collectionId == null) {
      setLoadingHistory(false);
      return;
    }
    if (scope === 'meeting' && !meetingId) {
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
        scope,
        collection_id: scope === 'collection' ? collectionId : null,
        meeting_id: scope === 'meeting' ? meetingId : null,
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
  }, [enabled, scope, collectionId, meetingId]);

  const startNewChat = useCallback(() => {
    setMessages([]);
    setSessionId(null);
    setInputState('');
    setHistoryIndex(null);
    draftBeforeHistory.current = '';
    inputRef.current?.focus();
  }, []);

  const send = useCallback(
    async (text: string) => {
      const query = text.trim();
      if (!query || sending || loadingHistory) return;

      if (scope === 'collection' && collectionId == null) {
        setMessages((m) => [...m, { role: 'assistant', content: t('Select a collection to search.'), error: true }]);
        return;
      }
      if (scope === 'meeting' && !meetingId) {
        setMessages((m) => [...m, { role: 'assistant', content: t('Select a meeting to search.'), error: true }]);
        return;
      }

      setInputState('');
      setHistoryIndex(null);
      draftBeforeHistory.current = '';
      setMessages((m) => [...m, { role: 'user', content: query }]);
      setSending(true);

      try {
        const res = await invoke<RagAskResponse>('rag_ask', {
          input: {
            query,
            scope,
            collection_id: scope === 'collection' ? collectionId : null,
            meeting_id: scope === 'meeting' ? meetingId : null,
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
            diagnostics: res.diagnostics,
            animate: true,
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
                : t('Failed to get an answer. Make sure OpenRouter is configured.'),
            error: true,
          },
        ]);
      } finally {
        setSending(false);
      }
    },
    [sending, loadingHistory, scope, collectionId, meetingId, sessionId, t],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        void send(input);
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
        setInputState(queryHistory[nextIndex]);
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
          setInputState(queryHistory[nextIndex]);
        } else {
          setHistoryIndex(null);
          setInputState(draftBeforeHistory.current);
        }
      }
    },
    [send, input, messages, historyIndex],
  );

  return {
    messages,
    setMessages,
    input,
    setInput,
    sending,
    loadingHistory,
    sessionId,
    historyIndex,
    send,
    onKeyDown,
    startNewChat,
    inputRef,
  };
}
