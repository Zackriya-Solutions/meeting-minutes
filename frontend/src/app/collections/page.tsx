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
import { Switch } from '@/components/ui/switch';
import { getCollectionDisplayText } from '@/lib/collectionDisplay';
import { useLanguage, useT } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';
import { cn } from '@/lib/utils';

interface CollectionRow {
  id: number;
  name: string;
  kind: 'manual' | 'series';
  meeting_count: number;
  auto_add: boolean;
  match_rule?: string | null;
  is_system: boolean;
  system_key?: string | null;
}

interface CollectionMeeting {
  id: string;
  title: string;
  created_at: string;
  occurred_at?: string | null;
  folder_path?: string | null;
  in_collection: boolean;
}

interface SeriesSuggestion {
  suggested_name: string;
  meeting_ids: string[];
  meetings: Array<{
    id: string;
    title: string;
    occurred_at: string;
  }>;
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

interface StandupSeriesInsight {
  kind: string;
  priority: 'high' | 'medium' | 'low';
  text: string;
  sources: SeriesDigestItem[];
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
  insights: StandupSeriesInsight[];
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

function InsightSection({ insights }: { insights: StandupSeriesInsight[] }) {
  const router = useRouter();
  const t = useT();
  if (insights.length === 0) return null;
  const labels: Record<string, string> = {
    action_missing_owner_and_due: t('Action is missing owner and due date'),
    action_missing_owner: t('Action is missing an owner'),
    action_missing_due: t('Action is missing a due date'),
    recurring_risk: t('Risk recurs across meetings'),
    carried_open_action: t('Action carried over from an earlier meeting'),
    unresolved_parking_lot: t('Topic remains in the parking lot'),
  };
  const priorityLabels: Record<StandupSeriesInsight['priority'], string> = {
    high: t('High priority'),
    medium: t('Medium priority'),
    low: t('Low priority'),
  };
  return (
    <section className="rounded-2xl border border-[var(--gold-border)] bg-[var(--gold-soft)] p-4 lg:col-span-2">
      <h4 className="text-xs font-medium uppercase tracking-[.12em] text-[var(--fg3)]">
        {t('Suggested follow-ups')}
      </h4>
      <p className="mt-1 text-xs text-[var(--fg3)]">
        {t('Derived locally from accepted records. Nothing is sent or changed automatically.')}
      </p>
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        {insights.map((insight, index) => {
          const source = insight.sources[0];
          const priorityClass = insight.priority === 'high'
            ? 'bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] text-[var(--danger)]'
            : insight.priority === 'medium'
              ? 'bg-[var(--gold-soft)] text-[var(--gold)]'
              : 'bg-[var(--bg-sheet)] text-[var(--fg3)]';
          const content = (
            <>
              <span className="flex items-center justify-between gap-2">
                <span className="block text-xs font-medium text-[var(--gold)]">
                  {labels[insight.kind] ?? t('Review accepted fact')}
                </span>
                <span className={cn('shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide', priorityClass)}>
                  {priorityLabels[insight.priority]}
                </span>
              </span>
              <span className="mt-1 block text-sm text-[var(--fg1)]">{insight.text}</span>
              {source ? (
                <span className="mt-1 block text-xs text-[var(--fg3)]">
                  {source.source_meeting_title}
                  {insight.sources.length > 1 ? ` · ${t('sources')}: ${insight.sources.length}` : ''}
                </span>
              ) : null}
            </>
          );
          return source ? (
            <button
              type="button"
              key={`${insight.kind}-${source.record_id}-${index}`}
              onClick={() => router.push(digestSourceHref(source))}
              className="rounded-xl bg-[var(--bg-elevated)] px-3 py-2.5 text-left hover:ring-1 hover:ring-[var(--gold-border)]"
            >
              {content}
            </button>
          ) : (
            <div key={`${insight.kind}-${index}`} className="rounded-xl bg-[var(--bg-elevated)] px-3 py-2.5">
              {content}
            </div>
          );
        })}
      </div>
    </section>
  );
}

export default function CollectionsPage() {
  const router = useRouter();
  const { t, lang } = useLanguage();
  const [collections, setCollections] = useState<CollectionRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [meetings, setMeetings] = useState<CollectionMeeting[]>([]);
  const [suggestions, setSuggestions] = useState<SeriesSuggestion[]>([]);
  const [dismissedSuggestions, setDismissedSuggestions] = useState<Set<string>>(new Set());
  const [expandedSuggestions, setExpandedSuggestions] = useState<Set<string>>(new Set());
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
  const [collectionSearch, setCollectionSearch] = useState('');
  const [meetingSearch, setMeetingSearch] = useState('');
  const [manageSearch, setManageSearch] = useState('');
  const [collectionQuestion, setCollectionQuestion] = useState('');
  const [savingAutoAdd, setSavingAutoAdd] = useState(false);
  const [convertOpen, setConvertOpen] = useState(false);
  const [converting, setConverting] = useState(false);

  const selected = useMemo(
    () => collections.find((collection) => collection.id === selectedId) ?? null,
    [collections, selectedId],
  );
  const selectedDisplay = useMemo(
    () => (selected ? getCollectionDisplayText(selected, t) : null),
    [selected, t],
  );
  const selectedMeetings = useMemo(
    () => meetings.filter((meeting) => meeting.in_collection),
    [meetings],
  );
  const normalizeSearch = (value: string) =>
    value.toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US').trim();
  const filteredCollections = useMemo(() => {
    const query = normalizeSearch(collectionSearch);
    if (!query) return collections;
    return collections.filter((collection) => {
      const displayName = getCollectionDisplayText(collection, t).name;
      return `${displayName} ${collection.name}`
        .toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US')
        .includes(query);
    });
  }, [collectionSearch, collections, lang, t]);
  const filteredSelectedMeetings = useMemo(() => {
    const query = normalizeSearch(meetingSearch);
    if (!query) return selectedMeetings;
    return selectedMeetings.filter((meeting) => {
      const display = getMeetingDisplayInfo({
        title: meeting.title,
        createdAt: meeting.created_at,
        occurredAt: meeting.occurred_at,
        folderPath: meeting.folder_path,
      }, lang);
      return `${meeting.title} ${display.title} ${display.dateLabel}`
        .toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US')
        .includes(query);
    });
  }, [meetingSearch, selectedMeetings, lang]);
  const filteredManageMeetings = useMemo(() => {
    const query = normalizeSearch(manageSearch);
    if (!query) return meetings;
    return meetings.filter((meeting) => {
      const display = getMeetingDisplayInfo({
        title: meeting.title,
        createdAt: meeting.created_at,
        occurredAt: meeting.occurred_at,
        folderPath: meeting.folder_path,
      }, lang);
      return `${meeting.title} ${display.title} ${display.dateLabel}`
        .toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US')
        .includes(query);
    });
  }, [manageSearch, meetings, lang]);
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
    const requestedCollectionId = Number(
      new URLSearchParams(window.location.search).get('collectionId'),
    );
    Promise.all([
      invoke<CollectionRow[]>('list_collections'),
      invoke<SeriesSuggestion[]>('suggest_meeting_series'),
    ])
      .then(([collectionRows, suggestionRows]) => {
        if (cancelled) return;
        const next = Array.isArray(collectionRows) ? collectionRows : [];
        setCollections(next);
        setSelectedId(
          Number.isInteger(requestedCollectionId)
            && next.some((collection) => collection.id === requestedCollectionId)
            ? requestedCollectionId
            : next[0]?.id ?? null,
        );
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
    setManageSearch('');
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
      toast.success(t('Series created'), {
        description: t('Future meetings with a matching title will be added automatically.'),
      });
    } catch (error) {
      toast.error(errorText(error, t('Failed to create series')));
    } finally {
      setAcceptingSuggestion(null);
    }
  };

  const toggleAutoAdd = async (enabled: boolean) => {
    if (!selected || selected.kind !== 'series' || savingAutoAdd) return;
    setSavingAutoAdd(true);
    try {
      const result = await invoke<{
        enabled: boolean;
        match_rule?: string | null;
        added_count: number;
      }>('set_series_auto_add', {
        collectionId: selected.id,
        enabled,
      });
      await loadCollections(selected.id);
      if (result.added_count > 0) {
        const rows = await invoke<CollectionMeeting[]>('list_collection_candidates', {
          collectionId: selected.id,
        });
        setMeetings(Array.isArray(rows) ? rows : []);
      }
      toast.success(enabled ? t('Automatic additions enabled') : t('Automatic additions disabled'), {
        description: result.added_count > 0
          ? `${t('Meetings added')}: ${result.added_count}`
          : undefined,
      });
    } catch (error) {
      toast.error(errorText(error, t('Failed to update automatic additions')));
    } finally {
      setSavingAutoAdd(false);
    }
  };

  const convertToSeries = async () => {
    if (!selected || selected.kind !== 'manual' || converting) return;
    setConverting(true);
    try {
      const result = await invoke<{
        enabled: boolean;
        match_rule?: string | null;
        added_count: number;
      }>('convert_collection_to_series', { collectionId: selected.id });
      setConvertOpen(false);
      await loadCollections(selected.id);
      if (result.added_count > 0) {
        const rows = await invoke<CollectionMeeting[]>('list_collection_candidates', {
          collectionId: selected.id,
        });
        setMeetings(Array.isArray(rows) ? rows : []);
      }
      toast.success(t('Collection converted to a recurring series'), {
        description: `${t('Automatic rule')}: “${result.match_rule || selected.name}”`,
      });
    } catch (error) {
      toast.error(errorText(error, t('Failed to convert collection to a series')));
    } finally {
      setConverting(false);
    }
  };

  const openCollectionChat = (question?: string) => {
    if (!selected) return;
    const params = new URLSearchParams({
      scope: 'collection',
      collectionId: String(selected.id),
      from: 'collection',
    });
    const normalizedQuestion = question?.trim();
    if (normalizedQuestion) params.set('question', normalizedQuestion);
    router.push(`/chat?${params.toString()}`);
  };

  const submitCollectionQuestion = () => {
    const question = collectionQuestion.trim();
    if (!question) return;
    openCollectionChat(question);
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
            <p className="mt-1 text-sm text-[var(--fg3)]">{t('Group recurring or related meetings, search inside them, and ask questions using only their content.')}</p>
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
          <label className="relative block">
            <Icon name="search" size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--fg3)]" />
            <input
              value={collectionSearch}
              onChange={(event) => setCollectionSearch(event.target.value)}
              className="mm-field h-10 w-full pl-9 text-sm outline-none"
              placeholder={t('Find a collection')}
            />
          </label>
          <div className="flex flex-col gap-1">
            {loading ? (
              <div className="px-3 py-6 text-sm text-[var(--fg3)]">{t('Loading…')}</div>
            ) : collections.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-[var(--border-subtle)] px-4 py-5 text-sm leading-relaxed text-[var(--fg3)]">
                {t('Create a collection to group meetings by project, client, or topic.')}
              </div>
            ) : filteredCollections.length === 0 ? (
              <div className="px-3 py-6 text-sm text-[var(--fg3)]">{t('No collections found')}</div>
            ) : (
              filteredCollections.map((collection) => {
                const display = getCollectionDisplayText(collection, t);
                return (
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
                      <span className="block truncate text-sm font-medium">{display.name}</span>
                      <span className="mt-0.5 block text-xs text-[var(--fg3)]">
                        {collection.meeting_count} {t('meetings')}
                        {collection.kind === 'series' && collection.auto_add ? ` · ${t('auto')}` : ''}
                      </span>
                    </span>
                  </button>
                );
              })
            )}
          </div>

          {visibleSuggestions.length > 0 && (
            <div className="mt-auto border-t border-[var(--border-subtle)] pt-4">
              <div className="mb-2 flex items-center gap-2 px-2 text-xs font-medium uppercase tracking-[.14em] text-[var(--fg3)]">
                <Icon name="spark" size={15} />
                {t('Suggested series')}
              </div>
              <p className="mb-3 px-2 text-xs leading-relaxed text-[var(--fg3)]">
                {t('Memento found recurring meetings. Nothing is created until you confirm.')}
              </p>
              <div className="flex flex-col gap-2">
                {visibleSuggestions.map((suggestion) => (
                  <div key={suggestion.suggested_name} className="rounded-2xl border border-[var(--gold-border)] bg-[var(--gold-soft)] p-3">
                    <div className="text-sm font-medium text-[var(--fg1)]">{suggestion.suggested_name}</div>
                    <div className="mt-1 text-xs text-[var(--fg3)]">
                      {suggestion.meeting_ids.length} {t('meetings')} · {t(suggestion.cadence)}
                    </div>
                    <button
                      type="button"
                      onClick={() => setExpandedSuggestions((current) => {
                        const next = new Set(current);
                        if (next.has(suggestion.suggested_name)) next.delete(suggestion.suggested_name);
                        else next.add(suggestion.suggested_name);
                        return next;
                      })}
                      aria-expanded={expandedSuggestions.has(suggestion.suggested_name)}
                      className="mt-2 flex w-full items-center justify-between rounded-xl px-2 py-1.5 text-left text-xs font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--gold-soft-strong)] hover:text-[var(--fg1)]"
                    >
                      <span className="flex items-center gap-1.5">
                        <Icon name="eye" size={14} />
                        {expandedSuggestions.has(suggestion.suggested_name) ? t('Hide meetings') : t('Show meetings')}
                      </span>
                      <Icon
                        name={expandedSuggestions.has(suggestion.suggested_name) ? 'chevron-up' : 'chevron-down'}
                        size={14}
                      />
                    </button>
                    {expandedSuggestions.has(suggestion.suggested_name) && (
                      <div className="mt-2 max-h-56 space-y-1 overflow-y-auto rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-1.5" role="list">
                        {suggestion.meetings.map((meeting) => {
                          const display = getMeetingDisplayInfo({
                            title: meeting.title,
                            occurredAt: meeting.occurred_at,
                          }, lang);
                          return (
                            <div
                              key={meeting.id}
                              role="listitem"
                              className="rounded-lg px-2 py-2 text-left"
                            >
                              <div className="break-words text-xs font-medium leading-snug text-[var(--fg1)]">
                                {display.title}
                              </div>
                              <div className="mt-0.5 flex items-center gap-1 text-[11px] text-[var(--fg3)]">
                                <Icon name="calendar" size={12} />
                                {display.dateLabel}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}
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
                    <Icon name={selected.system_key === 'inbox' ? 'spark' : selected.kind === 'series' ? 'refresh' : 'folder'} size={14} />
                    {selectedDisplay?.category}
                  </div>
                  <h2 className="truncate text-3xl font-semibold tracking-[-.04em]">{selectedDisplay?.name}</h2>
                  <p className="mt-2 text-sm text-[var(--fg3)]">
                    {selected.meeting_count} {t('meetings in this collection')}
                  </p>
                  <p className="mt-2 max-w-2xl text-sm leading-relaxed text-[var(--fg2)]">
                    {selectedDisplay?.description}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {!selected.is_system && <Button variant="secondary" onClick={openMembership} icon={<Icon name="plus" size={16} />}>
                    {t('Manage meetings')}
                  </Button>}
                  {!selected.is_system && selected.kind === 'manual' && selected.meeting_count >= 3 && (
                    <Button
                      variant="secondary"
                      onClick={() => setConvertOpen(true)}
                      icon={<Icon name="refresh" size={16} />}
                    >
                      {t('Make recurring')}
                    </Button>
                  )}
                  <Button
                    variant="secondary"
                    onClick={() => router.push(`/search?collectionId=${selected.id}`)}
                    icon={<Icon name="search" size={16} />}
                  >
                    {t('Search collection content')}
                  </Button>
                  {!selected.is_system && <button onClick={openRename} className="mm-icon-button" aria-label={t('Rename collection')}>
                    <Icon name="edit" size={17} />
                  </button>}
                  {!selected.is_system && <button onClick={() => setDeleteOpen(true)} className="mm-icon-button hover:text-[var(--danger)]" aria-label={t('Delete collection')}>
                    <Icon name="trash" size={17} />
                  </button>}
                </div>
              </div>

              {selected.kind === 'series' && (
                <div className="border-b border-[var(--border-subtle)] p-6">
                  <div className="flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4">
                    <div className="min-w-0">
                      <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Add future matching meetings automatically')}</h3>
                      <p className="mt-1 max-w-2xl text-sm leading-relaxed text-[var(--fg3)]">
                        {selected.auto_add
                          ? `${t('Enabled. Memento compares new or renamed meeting titles with this rule')}: “${selected.match_rule || selected.name}”.`
                          : t('Disabled. This series will keep its current meetings until you add more manually.')}
                      </p>
                      <p className="mt-1 text-xs text-[var(--fg3)]">
                        {t('No collection is ever created automatically. Memento only suggests a series; you decide whether to create it.')}
                      </p>
                    </div>
                    <Switch
                      checked={selected.auto_add}
                      disabled={savingAutoAdd}
                      onCheckedChange={toggleAutoAdd}
                      aria-label={t('Add future matching meetings automatically')}
                    />
                  </div>
                </div>
              )}

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
                        seriesDigest.highlights,
                        seriesDigest.updates,
                        seriesDigest.open_actions,
                        seriesDigest.done_actions,
                        seriesDigest.decisions,
                        seriesDigest.risks,
                        seriesDigest.deep_dives,
                        seriesDigest.parking_lot,
                      ].every((items) => items.length === 0) ? (
                        <div className="mt-3 rounded-2xl border border-dashed border-[var(--border-strong)] px-4 py-5 text-sm text-[var(--fg3)]">
                          {t('Review extracted records inside standup meetings to make the series digest trustworthy.')}
                        </div>
                      ) : (
                        <div className="mt-3 grid gap-3 lg:grid-cols-2">
                          <InsightSection insights={seriesDigest.insights} />
                          <DigestSection title={t('Highlights')} items={seriesDigest.highlights} />
                          <DigestSection title={t('Participant updates')} items={seriesDigest.updates} />
                          <DigestSection title={t('Open actions')} items={seriesDigest.open_actions} />
                          <DigestSection title={t('Completed actions')} items={seriesDigest.done_actions} />
                          <DigestSection title={t('Decisions')} items={seriesDigest.decisions} />
                          <DigestSection title={t('Risks and blockers')} items={seriesDigest.risks} />
                          <DigestSection title={t('Deep dives')} items={seriesDigest.deep_dives} />
                          <DigestSection title={t('Parking lot')} items={seriesDigest.parking_lot} />
                        </div>
                      )}
                    </div>
                  ) : null}
                </div>
              )}

              <div className="p-6">
                <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
                  <h3 className="text-sm font-medium uppercase tracking-[.12em] text-[var(--fg3)]">{t('Meetings')}</h3>
                  <label className="relative min-w-[240px] max-w-sm flex-1 sm:flex-none">
                    <Icon name="search" size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--fg3)]" />
                    <input
                      value={meetingSearch}
                      onChange={(event) => setMeetingSearch(event.target.value)}
                      className="mm-field h-10 w-full pl-9 text-sm outline-none"
                      placeholder={t('Find a meeting in this collection')}
                    />
                  </label>
                </div>
                {loadingMeetings ? (
                  <div className="py-12 text-center text-sm text-[var(--fg3)]">{t('Loading…')}</div>
                ) : selectedMeetings.length === 0 ? (
                  <button onClick={openMembership} className="flex w-full flex-col items-center rounded-3xl border border-dashed border-[var(--border-strong)] px-8 py-14 text-center hover:bg-[var(--bg-elevated)]">
                    <span className="mm-empty-icon"><Icon name="plus" size={24} /></span>
                    <span className="mt-4 text-base font-medium text-[var(--fg1)]">{t('Add meetings')}</span>
                    <span className="mt-1 max-w-sm text-sm text-[var(--fg3)]">{t('Choose recordings that belong to this project, client, or recurring series.')}</span>
                  </button>
                ) : filteredSelectedMeetings.length === 0 ? (
                  <div className="rounded-2xl border border-dashed border-[var(--border-strong)] px-4 py-10 text-center text-sm text-[var(--fg3)]">
                    {t('No meetings found in this collection')}
                  </div>
                ) : (
                  <div className="grid gap-2">
                    {filteredSelectedMeetings.map((meeting) => {
                      const display = getMeetingDisplayInfo({
                        title: meeting.title,
                        createdAt: meeting.created_at,
                        occurredAt: meeting.occurred_at,
                        folderPath: meeting.folder_path,
                      }, lang);
                      return (
                      <div key={meeting.id} className="group flex items-center gap-3 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3 hover:border-[var(--gold-border)]">
                        <button
                          onClick={() => router.push(`/meeting-details?id=${encodeURIComponent(meeting.id)}`)}
                          className="flex min-w-0 flex-1 items-center gap-3 text-left"
                        >
                          <span className="flex h-10 w-10 flex-none items-center justify-center rounded-xl bg-[var(--bg-elevated)] text-[var(--gold)]">
                            <Icon name="transcript" size={18} />
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm font-medium text-[var(--fg1)]">{display.title}</span>
                            <span className="mt-1 block text-xs text-[var(--fg3)]">{display.dateLabel}</span>
                          </span>
                          <Icon name="chevron-right" size={17} className="text-[var(--fg3)]" />
                        </button>
                        <button onClick={() => removeMeeting(meeting.id)} className="mm-icon-button h-9 w-9 opacity-0 group-hover:opacity-100" aria-label={t('Remove from collection')}>
                          <Icon name="close" size={15} />
                        </button>
                      </div>
                      );
                    })}
                  </div>
                )}

                <section className="mt-6 rounded-3xl border border-[var(--gold-border)] bg-[var(--gold-soft)] p-5">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="flex min-w-0 items-start gap-3">
                      <span className="flex h-10 w-10 flex-none items-center justify-center rounded-xl bg-[var(--gold-soft-strong)] text-[var(--gold)]">
                        <Icon name="chat" size={18} />
                      </span>
                      <div className="min-w-0">
                        <h3 className="text-base font-semibold text-[var(--fg1)]">{t('Ask this collection')}</h3>
                        <p className="mt-1 text-sm leading-relaxed text-[var(--fg3)]">
                          {t('The answer will use only meetings from this collection and include links to source moments.')}
                        </p>
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => openCollectionChat()}
                      icon={<Icon name="chat" size={15} />}
                    >
                      {t('Open previous conversation')}
                    </Button>
                  </div>
                  <div className="mt-4 flex items-end gap-2">
                    <textarea
                      value={collectionQuestion}
                      onChange={(event) => setCollectionQuestion(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' && !event.shiftKey) {
                          event.preventDefault();
                          submitCollectionQuestion();
                        }
                      }}
                      rows={1}
                      placeholder={t('Ask about this collection…')}
                      className="mm-field max-h-40 min-h-[48px] flex-1 resize-none bg-[var(--bg-surface)] py-3 text-sm outline-none"
                    />
                    <button
                      type="button"
                      onClick={submitCollectionQuestion}
                      disabled={!collectionQuestion.trim()}
                      className={cn(
                        'flex h-11 w-11 flex-none items-center justify-center rounded-xl text-[var(--fg-inverse)] transition-colors',
                        collectionQuestion.trim()
                          ? 'bg-[var(--gold)] hover:bg-[var(--gold-active)]'
                          : 'cursor-not-allowed bg-[var(--bg-elevated)]',
                      )}
                      aria-label={t('Ask this collection')}
                    >
                      <Icon name="send" size={18} />
                    </button>
                  </div>
                </section>
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
        <DialogContent className="max-h-[80vh] max-w-2xl grid-rows-[auto_auto_minmax(0,1fr)_auto]">
          <DialogHeader>
            <DialogTitle>{t('Manage meetings')}</DialogTitle>
            <DialogDescription>
              {membershipDraft.size} {t('meetings selected')}
            </DialogDescription>
          </DialogHeader>
          <label className="relative block">
            <Icon name="search" size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--fg3)]" />
            <input
              value={manageSearch}
              onChange={(event) => setManageSearch(event.target.value)}
              className="mm-field h-10 w-full pl-9 text-sm outline-none"
              placeholder={t('Find a meeting to add')}
            />
          </label>
          <div className="min-h-0 overflow-y-auto rounded-2xl border border-[var(--border-subtle)]">
            {meetings.length === 0 ? (
              <div className="p-8 text-center text-sm text-[var(--fg3)]">{t('There are no recorded meetings yet.')}</div>
            ) : filteredManageMeetings.length === 0 ? (
              <div className="p-8 text-center text-sm text-[var(--fg3)]">{t('No meetings found')}</div>
            ) : filteredManageMeetings.map((meeting) => {
              const display = getMeetingDisplayInfo({
                title: meeting.title,
                createdAt: meeting.created_at,
                occurredAt: meeting.occurred_at,
                folderPath: meeting.folder_path,
              }, lang);
              return (
              <label key={meeting.id} className="flex cursor-pointer items-center gap-3 border-b border-[var(--border-subtle)] px-4 py-3 last:border-b-0 hover:bg-[var(--bg-sheet)]">
                <input
                  type="checkbox"
                  checked={membershipDraft.has(meeting.id)}
                  onChange={() => toggleMeeting(meeting.id)}
                  className="h-4 w-4 accent-[var(--gold)]"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm text-[var(--fg1)]">{display.title}</span>
                  <span className="mt-0.5 block text-xs text-[var(--fg3)]">{display.dateLabel}</span>
                </span>
              </label>
              );
            })}
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

      <Dialog open={convertOpen} onOpenChange={setConvertOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('Make this a recurring series?')}</DialogTitle>
            <DialogDescription>
              {t('Memento will derive a matching rule from the meetings already selected, add matching archive meetings, and automatically add future matching meetings. You can turn automatic additions off later.')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setConvertOpen(false)}>{t('Cancel')}</Button>
            <Button onClick={convertToSeries} disabled={converting}>
              {converting ? t('Converting…') : t('Make recurring')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
