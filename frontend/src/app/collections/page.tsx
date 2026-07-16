'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { Button } from '@/components/memento/Button';
import { Icon } from '@/components/memento/Icon';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';

interface CollectionRow {
  id: number;
  name: string;
  kind: 'manual' | 'series';
  meeting_count: number;
}

interface CollectionMeeting {
  id: string;
  title: string;
  created_at: string;
  in_collection: boolean;
}

interface SeriesSuggestion {
  suggested_name: string;
  meeting_ids: string[];
  cadence: string;
}

interface SeriesDigestItem {
  record_id: number;
  kind: string;
  text: string;
  participant?: string | null;
  category?: string | null;
  owner?: string | null;
  due_date?: string | null;
  action_status?: string | null;
  parking_lot: boolean;
  source_meeting_id: string;
  source_meeting_title: string;
  source_occurred_at: string;
  source_start_ms?: number | null;
}

interface StandupSeriesDigest {
  collection_id: number;
  series_name: string;
  window_days?: number | null;
  period_start?: string | null;
  period_end?: string | null;
  meeting_count: number;
  meetings_with_accepted_records: number;
  pending_review_count: number;
  highlights: SeriesDigestItem[];
  updates: SeriesDigestItem[];
  decisions: SeriesDigestItem[];
  risks: SeriesDigestItem[];
  deep_dives: SeriesDigestItem[];
  parking_lot: SeriesDigestItem[];
  open_actions: SeriesDigestItem[];
  done_actions: SeriesDigestItem[];
  cancelled_actions: SeriesDigestItem[];
  markdown: string;
}

type EditorMode = 'create' | 'rename';

function formatMeetingDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value.slice(0, 10);
  return new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', year: 'numeric' }).format(date);
}

function errorText(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  return fallback;
}

function digestSourceHref(item: SeriesDigestItem): string {
  const base = `/meeting-details?id=${encodeURIComponent(item.source_meeting_id)}`;
  if (item.source_start_ms == null) return base;
  const seconds = Math.max(0, Math.floor(item.source_start_ms / 1000));
  return `${base}&t=${seconds}`;
}

function DigestSection({ title, items }: { title: string; items: SeriesDigestItem[] }) {
  const router = useRouter();
  const t = useT();
  if (items.length === 0) return null;
  return (
    <section className="rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4">
      <h4 className="text-xs font-medium uppercase tracking-[.12em] text-[var(--fg3)]">{title}</h4>
      <div className="mt-3 grid gap-2">
        {items.map((item) => {
          const content = (
            <>
            {item.category ? (
              <span className="mb-1 inline-flex rounded-full bg-[var(--gold-soft)] px-2 py-0.5 text-[10px] font-medium uppercase tracking-[.08em] text-[var(--gold)]">
                {item.category === 'blockers'
                  ? t('Blocker')
                  : item.category === 'next'
                    ? t('Next')
                    : t('Completed')}
              </span>
            ) : null}
            <span className="block text-[var(--fg1)]">
              {item.participant ? <strong>{item.participant}: </strong> : null}
              {item.text}
            </span>
            <span className="mt-1 block text-xs text-[var(--fg3)]">
              {[item.owner, item.due_date, item.source_meeting_title].filter(Boolean).join(' · ')}
            </span>
            </>
          );
          if (item.source_start_ms == null) {
            return (
              <div
                key={item.record_id}
                className="rounded-xl bg-[var(--bg-elevated)] px-3 py-2.5 text-left text-sm"
              >
                {content}
              </div>
            );
          }
          return (
            <button
              type="button"
              key={item.record_id}
              onClick={() => router.push(digestSourceHref(item))}
              className="rounded-xl bg-[var(--bg-elevated)] px-3 py-2.5 text-left text-sm hover:ring-1 hover:ring-[var(--gold-border)]"
            >
              {content}
            </button>
          );
        })}
      </div>
    </section>
  );
}

export default function CollectionsPage() {
  const router = useRouter();
  const t = useT();
  const [collections, setCollections] = useState<CollectionRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [meetings, setMeetings] = useState<CollectionMeeting[]>([]);
  const [suggestions, setSuggestions] = useState<SeriesSuggestion[]>([]);
  const [dismissedSuggestions, setDismissedSuggestions] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [loadingMeetings, setLoadingMeetings] = useState(false);

  const [editorOpen, setEditorOpen] = useState(false);
  const [editorMode, setEditorMode] = useState<EditorMode>('create');
  const [editorName, setEditorName] = useState('');
  const [savingEditor, setSavingEditor] = useState(false);

  const [manageOpen, setManageOpen] = useState(false);
  const [membershipDraft, setMembershipDraft] = useState<Set<string>>(new Set());
  const [savingMembership, setSavingMembership] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [acceptingSuggestion, setAcceptingSuggestion] = useState<string | null>(null);
  const [digestWindowDays, setDigestWindowDays] = useState<number | null>(14);
  const [seriesDigest, setSeriesDigest] = useState<StandupSeriesDigest | null>(null);
  const [loadingDigest, setLoadingDigest] = useState(false);

  const selected = useMemo(
    () => collections.find((collection) => collection.id === selectedId) ?? null,
    [collections, selectedId],
  );
  const selectedMeetings = useMemo(
    () => meetings.filter((meeting) => meeting.in_collection),
    [meetings],
  );
  const visibleSuggestions = suggestions.filter(
    (suggestion) => !dismissedSuggestions.has(suggestion.suggested_name),
  );

  const loadCollections = useCallback(async (preferredId?: number) => {
    const rows = await invoke<CollectionRow[]>('list_collections');
    const next = Array.isArray(rows) ? rows : [];
    setCollections(next);
    setSelectedId((current) => {
      if (preferredId && next.some((item) => item.id === preferredId)) return preferredId;
      if (current && next.some((item) => item.id === current)) return current;
      return next[0]?.id ?? null;
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<CollectionRow[]>('list_collections'),
      invoke<SeriesSuggestion[]>('suggest_meeting_series'),
    ])
      .then(([collectionRows, suggestionRows]) => {
        if (cancelled) return;
        const next = Array.isArray(collectionRows) ? collectionRows : [];
        setCollections(next);
        setSelectedId(next[0]?.id ?? null);
        setSuggestions(Array.isArray(suggestionRows) ? suggestionRows : []);
      })
      .catch((error) => {
        if (!cancelled) toast.error(errorText(error, t('Failed to load collections')));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  useEffect(() => {
    if (selectedId == null) {
      setMeetings([]);
      return;
    }
    let cancelled = false;
    setLoadingMeetings(true);
    invoke<CollectionMeeting[]>('list_collection_candidates', { collectionId: selectedId })
      .then((rows) => {
        if (!cancelled) setMeetings(Array.isArray(rows) ? rows : []);
      })
      .catch((error) => {
        if (!cancelled) toast.error(errorText(error, t('Failed to load meetings')));
      })
      .finally(() => {
        if (!cancelled) setLoadingMeetings(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, t]);

  useEffect(() => {
    if (selectedId == null || selected?.kind !== 'series') {
      setSeriesDigest(null);
      return;
    }
    let cancelled = false;
    setLoadingDigest(true);
    invoke<StandupSeriesDigest>('get_standup_series_digest', {
      collectionId: selectedId,
      windowDays: digestWindowDays,
      outputLanguage: typeof navigator === 'undefined' ? 'en' : navigator.language,
    })
      .then((digest) => {
        if (!cancelled) setSeriesDigest(digest);
      })
      .catch((error) => {
        if (!cancelled) {
          setSeriesDigest(null);
          toast.error(errorText(error, t('Failed to build standup digest')));
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingDigest(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, selected?.kind, digestWindowDays, t]);

  const copySeriesDigest = async () => {
    if (!seriesDigest?.markdown) return;
    try {
      await navigator.clipboard.writeText(seriesDigest.markdown);
      toast.success(t('Digest copied'));
    } catch (error) {
      toast.error(errorText(error, t('Failed to copy digest')));
    }
  };

  const openCreate = () => {
    setEditorMode('create');
    setEditorName('');
    setEditorOpen(true);
  };

  const openRename = () => {
    if (!selected) return;
    setEditorMode('rename');
    setEditorName(selected.name);
    setEditorOpen(true);
  };

  const saveEditor = async () => {
    const name = editorName.trim();
    if (!name || savingEditor) return;
    setSavingEditor(true);
    try {
      if (editorMode === 'create') {
        const id = await invoke<number>('create_collection', { name, kind: 'manual' });
        await loadCollections(id);
        toast.success(t('Collection created'));
      } else if (selectedId != null) {
        await invoke('rename_collection', { collectionId: selectedId, name });
        await loadCollections(selectedId);
        toast.success(t('Collection renamed'));
      }
      setEditorOpen(false);
    } catch (error) {
      toast.error(errorText(error, t('Failed to save collection')));
    } finally {
      setSavingEditor(false);
    }
  };

  const openMembership = () => {
    setMembershipDraft(new Set(selectedMeetings.map((meeting) => meeting.id)));
    setManageOpen(true);
  };

  const toggleMeeting = (meetingId: string) => {
    setMembershipDraft((current) => {
      const next = new Set(current);
      if (next.has(meetingId)) next.delete(meetingId);
      else next.add(meetingId);
      return next;
    });
  };

  const saveMembership = async () => {
    if (selectedId == null || savingMembership) return;
    setSavingMembership(true);
    try {
      await invoke('set_collection_meetings', {
        collectionId: selectedId,
        meetingIds: Array.from(membershipDraft),
      });
      const rows = await invoke<CollectionMeeting[]>('list_collection_candidates', {
        collectionId: selectedId,
      });
      setMeetings(Array.isArray(rows) ? rows : []);
      await loadCollections(selectedId);
      setManageOpen(false);
      toast.success(t('Collection updated'));
    } catch (error) {
      toast.error(errorText(error, t('Failed to update collection')));
    } finally {
      setSavingMembership(false);
    }
  };

  const removeMeeting = async (meetingId: string) => {
    if (selectedId == null) return;
    const nextIds = selectedMeetings.filter((meeting) => meeting.id !== meetingId).map((meeting) => meeting.id);
    try {
      await invoke('set_collection_meetings', { collectionId: selectedId, meetingIds: nextIds });
      setMeetings((current) =>
        current.map((meeting) =>
          meeting.id === meetingId ? { ...meeting, in_collection: false } : meeting,
        ),
      );
      await loadCollections(selectedId);
    } catch (error) {
      toast.error(errorText(error, t('Failed to update collection')));
    }
  };

  const deleteSelected = async () => {
    if (selectedId == null || deleting) return;
    setDeleting(true);
    try {
      await invoke('delete_collection', { collectionId: selectedId });
      setDeleteOpen(false);
      await loadCollections();
      toast.success(t('Collection deleted'));
    } catch (error) {
      toast.error(errorText(error, t('Failed to delete collection')));
    } finally {
      setDeleting(false);
    }
  };

  const acceptSuggestion = async (suggestion: SeriesSuggestion) => {
    if (acceptingSuggestion) return;
    setAcceptingSuggestion(suggestion.suggested_name);
    try {
      const id = await invoke<number>('accept_series_suggestion', {
        suggestedName: suggestion.suggested_name,
        meetingIds: suggestion.meeting_ids,
      });
      setDismissedSuggestions((current) => new Set(current).add(suggestion.suggested_name));
      await loadCollections(id);
      toast.success(t('Series created'));
    } catch (error) {
      toast.error(errorText(error, t('Failed to create series')));
    } finally {
      setAcceptingSuggestion(null);
    }
  };

  return (
    <div className="mm-page min-w-0">
      <header className="mm-page-header justify-between">
        <div className="flex min-w-0 items-center gap-3">
          <button onClick={() => router.push('/')} className="mm-icon-button" aria-label={t('Back')}>
            <Icon name="back" />
          </button>
          <div className="min-w-0">
            <h1 className="mm-page-title">{t('Collections')}</h1>
            <p className="mt-1 text-sm text-[var(--fg3)]">{t('Organize related meetings and ask questions across them')}</p>
          </div>
        </div>
        <Button onClick={openCreate} icon={<Icon name="plus" size={17} />}>
          {t('New collection')}
        </Button>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(220px,280px)_minmax(0,1fr)] gap-5 pt-5 max-[820px]:grid-cols-1">
        <aside className="flex min-h-0 flex-col gap-4 overflow-y-auto rounded-3xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
          <div className="px-2 pt-1 text-xs font-medium uppercase tracking-[.14em] text-[var(--fg3)]">
            {t('Your collections')}
          </div>
          <div className="flex flex-col gap-1">
            {loading ? (
              <div className="px-3 py-6 text-sm text-[var(--fg3)]">{t('Loading…')}</div>
            ) : collections.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-[var(--border-subtle)] px-4 py-5 text-sm leading-relaxed text-[var(--fg3)]">
                {t('Create a collection to group meetings by project, client, or topic.')}
              </div>
            ) : (
              collections.map((collection) => (
                <button
                  key={collection.id}
                  onClick={() => setSelectedId(collection.id)}
                  className={cn(
                    'flex items-center gap-3 rounded-2xl px-3 py-3 text-left transition-colors',
                    selectedId === collection.id
                      ? 'bg-[var(--gold-soft)] text-[var(--fg1)]'
                      : 'text-[var(--fg2)] hover:bg-[var(--bg-elevated)]',
                  )}
                >
                  <span className={cn('flex h-9 w-9 items-center justify-center rounded-xl', selectedId === collection.id ? 'bg-[var(--gold-soft-strong)] text-[var(--gold)]' : 'bg-[var(--bg-elevated)]')}>
                    <Icon name={collection.kind === 'series' ? 'refresh' : 'folder'} size={18} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{collection.name}</span>
                    <span className="mt-0.5 block text-xs text-[var(--fg3)]">
                      {collection.meeting_count} {t('meetings')}
                    </span>
                  </span>
                </button>
              ))
            )}
          </div>

          {visibleSuggestions.length > 0 && (
            <div className="mt-auto border-t border-[var(--border-subtle)] pt-4">
              <div className="mb-2 flex items-center gap-2 px-2 text-xs font-medium uppercase tracking-[.14em] text-[var(--fg3)]">
                <Icon name="spark" size={15} />
                {t('Suggested series')}
              </div>
              <div className="flex flex-col gap-2">
                {visibleSuggestions.map((suggestion) => (
                  <div key={suggestion.suggested_name} className="rounded-2xl border border-[var(--gold-border)] bg-[var(--gold-soft)] p-3">
                    <div className="text-sm font-medium text-[var(--fg1)]">{suggestion.suggested_name}</div>
                    <div className="mt-1 text-xs text-[var(--fg3)]">
                      {suggestion.meeting_ids.length} {t('meetings')} · {t(suggestion.cadence)}
                    </div>
                    <div className="mt-3 flex gap-2">
                      <Button
                        size="sm"
                        onClick={() => acceptSuggestion(suggestion)}
                        disabled={acceptingSuggestion === suggestion.suggested_name}
                      >
                        {acceptingSuggestion === suggestion.suggested_name ? t('Creating…') : t('Create series')}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setDismissedSuggestions((current) => new Set(current).add(suggestion.suggested_name))}
                      >
                        {t('Dismiss')}
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </aside>

        <main className="min-h-0 min-w-0 overflow-y-auto rounded-3xl border border-[var(--border-subtle)] bg-[var(--bg-surface)]">
          {!selected ? (
            <div className="flex h-full min-h-[360px] flex-col items-center justify-center px-8 text-center">
              <div className="mm-empty-icon"><Icon name="folder" size={26} /></div>
              <h2 className="mt-4 text-xl font-semibold">{t('No collection selected')}</h2>
              <p className="mt-2 max-w-sm text-sm leading-relaxed text-[var(--fg3)]">
                {t('Create a collection and add meetings to build a focused knowledge space.')}
              </p>
              <Button className="mt-5" onClick={openCreate} icon={<Icon name="plus" size={17} />}>
                {t('New collection')}
              </Button>
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-start justify-between gap-4 border-b border-[var(--border-subtle)] p-6">
                <div className="min-w-0">
                  <div className="mb-2 flex items-center gap-2 text-xs uppercase tracking-[.12em] text-[var(--fg3)]">
                    <Icon name={selected.kind === 'series' ? 'refresh' : 'folder'} size={14} />
                    {selected.kind === 'series' ? t('Automatic series') : t('Manual collection')}
                  </div>
                  <h2 className="truncate text-3xl font-semibold tracking-[-.04em]">{selected.name}</h2>
                  <p className="mt-2 text-sm text-[var(--fg3)]">
                    {selected.meeting_count} {t('meetings in this collection')}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button variant="secondary" onClick={openMembership} icon={<Icon name="plus" size={16} />}>
                    {t('Manage meetings')}
                  </Button>
                  <Button
                    onClick={() => router.push(`/chat?scope=collection&collectionId=${selected.id}`)}
                    icon={<Icon name="chat" size={16} />}
                  >
                    {t('Ask this collection')}
                  </Button>
                  <button onClick={openRename} className="mm-icon-button" aria-label={t('Rename collection')}>
                    <Icon name="edit" size={17} />
                  </button>
                  <button onClick={() => setDeleteOpen(true)} className="mm-icon-button hover:text-[var(--danger)]" aria-label={t('Delete collection')}>
                    <Icon name="trash" size={17} />
                  </button>
                </div>
              </div>

              {selected.kind === 'series' && (
                <div className="border-b border-[var(--border-subtle)] p-6">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <h3 className="text-base font-semibold text-[var(--fg1)]">{t('Standup series digest')}</h3>
                      <p className="mt-1 text-sm text-[var(--fg3)]">
                        {t('Built only from accepted records, with links back to transcript evidence.')}
                      </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <div className="flex rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-1">
                        {[
                          { label: '7d', value: 7 },
                          { label: '14d', value: 14 },
                          { label: '30d', value: 30 },
                          { label: t('All'), value: null },
                        ].map((option) => (
                          <button
                            key={option.label}
                            onClick={() => setDigestWindowDays(option.value)}
                            className={cn(
                              'rounded-lg px-2.5 py-1.5 text-xs transition-colors',
                              digestWindowDays === option.value
                                ? 'bg-[var(--gold-soft-strong)] text-[var(--fg1)]'
                                : 'text-[var(--fg3)] hover:text-[var(--fg1)]',
                            )}
                          >
                            {option.label}
                          </button>
                        ))}
                      </div>
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={copySeriesDigest}
                        disabled={!seriesDigest?.markdown}
                        icon={<Icon name="copy" size={15} />}
                      >
                        {t('Copy digest')}
                      </Button>
                    </div>
                  </div>

                  {loadingDigest ? (
                    <div className="py-8 text-center text-sm text-[var(--fg3)]">{t('Building digest…')}</div>
                  ) : seriesDigest ? (
                    <div className="mt-5">
                      <div className="grid gap-2 sm:grid-cols-3">
                        <div className="rounded-2xl bg-[var(--bg-sheet)] p-3">
                          <div className="text-2xl font-semibold">{seriesDigest.meeting_count}</div>
                          <div className="mt-1 text-xs text-[var(--fg3)]">{t('meetings in window')}</div>
                        </div>
                        <div className="rounded-2xl bg-[var(--bg-sheet)] p-3">
                          <div className="text-2xl font-semibold">{seriesDigest.meetings_with_accepted_records}</div>
                          <div className="mt-1 text-xs text-[var(--fg3)]">{t('reviewed meetings')}</div>
                        </div>
                        <div className={cn('rounded-2xl p-3', seriesDigest.pending_review_count > 0 ? 'bg-[var(--gold-soft)]' : 'bg-[var(--bg-sheet)]')}>
                          <div className="text-2xl font-semibold">{seriesDigest.pending_review_count}</div>
                          <div className="mt-1 text-xs text-[var(--fg3)]">{t('records pending review')}</div>
                        </div>
                      </div>

                      {[
                        seriesDigest.open_actions,
                        seriesDigest.done_actions,
                        seriesDigest.decisions,
                        seriesDigest.risks,
                        seriesDigest.updates,
                        seriesDigest.highlights,
                        seriesDigest.deep_dives,
                        seriesDigest.parking_lot,
                      ].every((items) => items.length === 0) ? (
                        <div className="mt-3 rounded-2xl border border-dashed border-[var(--border-strong)] px-4 py-5 text-sm text-[var(--fg3)]">
                          {t('Review extracted records inside standup meetings to make the series digest trustworthy.')}
                        </div>
                      ) : (
                        <div className="mt-3 grid gap-3 lg:grid-cols-2">
                          <DigestSection title={t('Open actions')} items={seriesDigest.open_actions} />
                          <DigestSection title={t('Completed actions')} items={seriesDigest.done_actions} />
                          <DigestSection title={t('Decisions')} items={seriesDigest.decisions} />
                          <DigestSection title={t('Risks and blockers')} items={seriesDigest.risks} />
                          <DigestSection title={t('Participant updates')} items={seriesDigest.updates} />
                          <DigestSection title={t('Highlights')} items={seriesDigest.highlights} />
                          <DigestSection title={t('Deep dives')} items={seriesDigest.deep_dives} />
                          <DigestSection title={t('Parking lot')} items={seriesDigest.parking_lot} />
                        </div>
                      )}
                    </div>
                  ) : null}
                </div>
              )}

              <div className="p-6">
                <div className="mb-4 flex items-center justify-between gap-3">
                  <h3 className="text-sm font-medium uppercase tracking-[.12em] text-[var(--fg3)]">{t('Meetings')}</h3>
                  {selectedMeetings.length > 0 && (
                    <span className="text-xs text-[var(--fg3)]">{t('Newest first')}</span>
                  )}
                </div>
                {loadingMeetings ? (
                  <div className="py-12 text-center text-sm text-[var(--fg3)]">{t('Loading…')}</div>
                ) : selectedMeetings.length === 0 ? (
                  <button onClick={openMembership} className="flex w-full flex-col items-center rounded-3xl border border-dashed border-[var(--border-strong)] px-8 py-14 text-center hover:bg-[var(--bg-elevated)]">
                    <span className="mm-empty-icon"><Icon name="plus" size={24} /></span>
                    <span className="mt-4 text-base font-medium text-[var(--fg1)]">{t('Add meetings')}</span>
                    <span className="mt-1 max-w-sm text-sm text-[var(--fg3)]">{t('Choose recordings that belong to this project, client, or recurring series.')}</span>
                  </button>
                ) : (
                  <div className="grid gap-2">
                    {selectedMeetings.map((meeting) => (
                      <div key={meeting.id} className="group flex items-center gap-3 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3 hover:border-[var(--gold-border)]">
                        <button
                          onClick={() => router.push(`/meeting-details?id=${encodeURIComponent(meeting.id)}`)}
                          className="flex min-w-0 flex-1 items-center gap-3 text-left"
                        >
                          <span className="flex h-10 w-10 flex-none items-center justify-center rounded-xl bg-[var(--bg-elevated)] text-[var(--gold)]">
                            <Icon name="transcript" size={18} />
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm font-medium text-[var(--fg1)]">{meeting.title || t('Untitled')}</span>
                            <span className="mt-1 block text-xs text-[var(--fg3)]">{formatMeetingDate(meeting.created_at)}</span>
                          </span>
                          <Icon name="chevron-right" size={17} className="text-[var(--fg3)]" />
                        </button>
                        <button onClick={() => removeMeeting(meeting.id)} className="mm-icon-button h-9 w-9 opacity-0 group-hover:opacity-100" aria-label={t('Remove from collection')}>
                          <Icon name="close" size={15} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}
        </main>
      </div>

      <Dialog open={editorOpen} onOpenChange={setEditorOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editorMode === 'create' ? t('New collection') : t('Rename collection')}</DialogTitle>
            <DialogDescription>{t('Use a name that makes this group easy to find later.')}</DialogDescription>
          </DialogHeader>
          <input
            autoFocus
            value={editorName}
            maxLength={120}
            onChange={(event) => setEditorName(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && saveEditor()}
            className="mm-field w-full text-sm outline-none"
            placeholder={t('For example: Product launch')}
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setEditorOpen(false)}>{t('Cancel')}</Button>
            <Button onClick={saveEditor} disabled={!editorName.trim() || savingEditor}>
              {savingEditor ? t('Saving…') : t('Save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={manageOpen} onOpenChange={setManageOpen}>
        <DialogContent className="max-h-[80vh] max-w-2xl grid-rows-[auto_minmax(0,1fr)_auto]">
          <DialogHeader>
            <DialogTitle>{t('Manage meetings')}</DialogTitle>
            <DialogDescription>
              {membershipDraft.size} {t('meetings selected')}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 overflow-y-auto rounded-2xl border border-[var(--border-subtle)]">
            {meetings.length === 0 ? (
              <div className="p-8 text-center text-sm text-[var(--fg3)]">{t('There are no recorded meetings yet.')}</div>
            ) : meetings.map((meeting) => (
              <label key={meeting.id} className="flex cursor-pointer items-center gap-3 border-b border-[var(--border-subtle)] px-4 py-3 last:border-b-0 hover:bg-[var(--bg-sheet)]">
                <input
                  type="checkbox"
                  checked={membershipDraft.has(meeting.id)}
                  onChange={() => toggleMeeting(meeting.id)}
                  className="h-4 w-4 accent-[var(--gold)]"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm text-[var(--fg1)]">{meeting.title || t('Untitled')}</span>
                  <span className="mt-0.5 block text-xs text-[var(--fg3)]">{formatMeetingDate(meeting.created_at)}</span>
                </span>
              </label>
            ))}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setManageOpen(false)}>{t('Cancel')}</Button>
            <Button onClick={saveMembership} disabled={savingMembership}>
              {savingMembership ? t('Saving…') : t('Save selection')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('Delete collection?')}</DialogTitle>
            <DialogDescription>
              {t('The meetings and their recordings will stay in your archive. Only this collection will be deleted.')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleteOpen(false)}>{t('Cancel')}</Button>
            <Button onClick={deleteSelected} disabled={deleting} className="bg-[var(--danger)] hover:bg-[var(--danger)]">
              {deleting ? t('Deleting…') : t('Delete collection')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
