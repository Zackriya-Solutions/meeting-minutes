'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { Loader2 } from '@/components/memento/LucideCompat';
import { cn } from '@/lib/utils';
import { Icon } from '@/components/memento/Icon';
import { Button } from '@/components/memento/Button';
import { getCollectionDisplayText } from '@/lib/collectionDisplay';
import { useT } from '@/lib/i18n';
import { KnowledgeReadinessCard } from '@/components/KnowledgeReadinessCard';
import { useMeetingChat, type Citation, type ScopeKind } from '@/hooks/useMeetingChat';
import { MessageBubble, TypingIndicator } from '@/components/chat/MessageBubble';

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

  const [scopeInitialized, setScopeInitialized] = useState(false);
  const [scopeKind, setScopeKind] = useState<ScopeKind>('archive');
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [returnCollectionId, setReturnCollectionId] = useState<number | null>(null);

  const [meetings, setMeetings] = useState<MeetingRef[]>([]);
  const [collections, setCollections] = useState<CollectionRef[]>([]);

  const pendingQuestion = useRef<string | null>(null);
  const submittedPendingQuestion = useRef(false);

  const chat = useMeetingChat({
    scope: scopeKind,
    collectionId,
    meetingId,
    enabled: scopeInitialized,
  });
  const { messages, input, setInput, sending, loadingHistory, send, onKeyDown, startNewChat, scrollRef, inputRef } = chat;

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
    setScopeInitialized(true);
  }, []);

  // Changing scope restores the latest session for that scope (handled by the hook).
  const changeScope = (kind: ScopeKind) => {
    setScopeKind(kind);
    if (kind !== 'collection') setCollectionId(null);
    if (kind !== 'meeting') setMeetingId(null);
  };

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
  const selectedCollection = collections.find((collection) => collection.id === collectionId);
  const selectedCollectionName = selectedCollection ? getCollectionDisplayText(selectedCollection, t).name : null;
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
