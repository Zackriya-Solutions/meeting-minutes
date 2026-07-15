"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Check, ChevronDown, Pencil, RefreshCw, X } from '@/components/memento/LucideCompat';
import { getStandupLiveState, type StandupLiveState } from '@/lib/standupLiveState';

type ReviewStatus = 'pending' | 'accepted' | 'rejected';

interface EvidenceRef {
  timestamp: string;
  quote?: string | null;
}

interface StandupRecordRow {
  id: number;
  meeting_id: string;
  kind: string;
  payload: Record<string, any>;
  reviewed_payload?: Record<string, any> | null;
  review_status: ReviewStatus;
  action_item_id?: number | null;
  action_status?: string | null;
}

interface PrebriefAction {
  id: number;
  text: string;
  owner?: string | null;
  due_date?: string | null;
  status: string;
  source_meeting_id: string;
  source_meeting_title: string;
  source_start_ms?: number | null;
}

interface PrebriefFact {
  record_id: number;
  kind: string;
  text: string;
  source_meeting_id: string;
  source_meeting_title: string;
  source_start_ms?: number | null;
}

interface StandupPrebrief {
  series: string[];
  open_actions: PrebriefAction[];
  recent_risks: PrebriefFact[];
  recent_decisions: PrebriefFact[];
}

type PrivateNoteKind = 'planned_update' | 'parking_lot' | 'private_note';

interface StandupPrivateNote {
  id: number;
  meeting_id: string;
  kind: PrivateNoteKind;
  text: string;
  status: 'open' | 'done';
}

interface Draft {
  text: string;
  owner: string;
  dueDate: string;
}

const EMPTY_PREBRIEF: StandupPrebrief = {
  series: [],
  open_actions: [],
  recent_risks: [],
  recent_decisions: [],
};

function effectivePayload(record: StandupRecordRow): Record<string, any> {
  return record.reviewed_payload ?? record.payload;
}

function primaryText(record: StandupRecordRow): string {
  const payload = effectivePayload(record);
  switch (record.kind) {
    case 'decision': return payload.decision ?? '';
    case 'action': return payload.task ?? '';
    case 'risk': return payload.blocker_or_risk ?? '';
    case 'deep_dive': return payload.topic ?? '';
    default: return payload.text ?? '';
  }
}

function evidence(record: StandupRecordRow): EvidenceRef[] {
  const value = effectivePayload(record).evidence;
  return Array.isArray(value) ? value : [];
}

function evidenceHref(meetingId: string, timestamp: string): string | null {
  const match = /^\[(\d+):(\d{2})\]$/.exec(timestamp.trim());
  if (!match) return null;
  const seconds = Number(match[1]) * 60 + Number(match[2]);
  return `/meeting-details?id=${encodeURIComponent(meetingId)}&t=${seconds}`;
}

function sourceHref(meetingId: string, startMs?: number | null): string {
  const seconds = Math.max(0, Math.floor((startMs ?? 0) / 1000));
  return `/meeting-details?id=${encodeURIComponent(meetingId)}&t=${seconds}`;
}

export function StandupWorkflowPanel({
  meetingId,
  summaryStatus,
}: {
  meetingId: string;
  summaryStatus: string;
}) {
  const t = useT();
  const router = useRouter();
  const [records, setRecords] = useState<StandupRecordRow[]>([]);
  const [prebrief, setPrebrief] = useState<StandupPrebrief>(EMPTY_PREBRIEF);
  const [privateNotes, setPrivateNotes] = useState<StandupPrivateNote[]>([]);
  const [newNoteKind, setNewNoteKind] = useState<PrivateNoteKind>('planned_update');
  const [newNoteText, setNewNoteText] = useState('');
  const [noteBusy, setNoteBusy] = useState(false);
  const [liveState, setLiveState] = useState<StandupLiveState>(() =>
    getStandupLiveState(meetingId)
  );
  const noteBusyRef = useRef(false);
  const [loading, setLoading] = useState(true);
  const [busyRecordId, setBusyRecordId] = useState<number | null>(null);
  const [busyActionId, setBusyActionId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [draft, setDraft] = useState<Draft>({ text: '', owner: '', dueDate: '' });

  const refresh = useCallback(async () => {
    setLiveState(getStandupLiveState(meetingId));
    try {
      const [nextRecords, nextPrebrief, nextPrivateNotes] = await Promise.all([
        invoke<StandupRecordRow[]>('list_standup_records', { meetingId }),
        invoke<StandupPrebrief>('get_standup_prebrief', { meetingId }),
        invoke<StandupPrivateNote[]>('list_standup_private_notes', { meetingId }),
      ]);
      setRecords(nextRecords);
      setPrebrief(nextPrebrief);
      setPrivateNotes(nextPrivateNotes);
    } catch (error) {
      console.error('Failed to load standup workflow:', error);
    } finally {
      setLoading(false);
    }
  }, [meetingId]);

  useEffect(() => {
    setLoading(true);
    void refresh();
    const delayedRefresh = summaryStatus === 'completed'
      ? window.setTimeout(() => void refresh(), 1_000)
      : null;
    return () => {
      if (delayedRefresh !== null) window.clearTimeout(delayedRefresh);
    };
  }, [refresh, summaryStatus]);

  const pendingCount = useMemo(
    () => records.filter((record) => record.review_status === 'pending').length,
    [records],
  );

  const beginEdit = (record: StandupRecordRow) => {
    const payload = effectivePayload(record);
    setEditingId(record.id);
    setDraft({
      text: primaryText(record),
      owner: payload.owner ?? '',
      dueDate: payload.due_date ?? '',
    });
  };

  const review = async (record: StandupRecordRow, status: ReviewStatus, includeEdits = false) => {
    setBusyRecordId(record.id);
    try {
      await invoke('review_standup_record', {
        input: {
          recordId: record.id,
          status,
          ...(includeEdits ? {
            editedText: draft.text,
            owner: record.kind === 'action' || record.kind === 'risk' ? draft.owner : undefined,
            dueDate: record.kind === 'action' ? draft.dueDate : undefined,
          } : {}),
        },
      });
      setEditingId(null);
      await refresh();
    } catch (error) {
      console.error('Failed to review standup record:', error);
      toast.error(t('Failed to save standup review'));
    } finally {
      setBusyRecordId(null);
    }
  };

  const setActionStatus = async (actionItemId: number, status: 'open' | 'done' | 'cancelled') => {
    setBusyActionId(actionItemId);
    try {
      await invoke('set_standup_action_status', { actionItemId, status });
      await refresh();
    } catch (error) {
      console.error('Failed to update standup action:', error);
      toast.error(t('Failed to update action'));
    } finally {
      setBusyActionId(null);
    }
  };

  const createPrivateNote = async () => {
    if (noteBusyRef.current) return;
    const text = newNoteText.trim();
    if (!text) return;
    noteBusyRef.current = true;
    setNoteBusy(true);
    try {
      await invoke('create_standup_private_note', {
        input: { meetingId, kind: newNoteKind, text },
      });
      setNewNoteText('');
      await refresh();
    } catch (error) {
      console.error('Failed to save private standup note:', error);
      toast.error(t('Failed to save private note'));
    } finally {
      noteBusyRef.current = false;
      setNoteBusy(false);
    }
  };

  const setPrivateNoteStatus = async (noteId: number, status: 'open' | 'done' | 'archived') => {
    if (noteBusyRef.current) return;
    noteBusyRef.current = true;
    setNoteBusy(true);
    try {
      await invoke('set_standup_private_note_status', { noteId, status });
      await refresh();
    } catch (error) {
      console.error('Failed to update private standup note:', error);
      toast.error(t('Failed to update private note'));
    } finally {
      noteBusyRef.current = false;
      setNoteBusy(false);
    }
  };

  const hasPrebrief = prebrief.series.length > 0;
  const hasLiveState = liveState.enabled || liveState.completedUpdates > 0 || liveState.markers.length > 0;
  if (loading || (!hasPrebrief && records.length === 0 && privateNotes.length === 0 && !hasLiveState)) return null;

  const kindLabel = (kind: string) => {
    const labels: Record<string, string> = {
      overview: t('Outcome'),
      participant_update: t('Participant update'),
      decision: t('Decision'),
      action: t('Action'),
      risk: t('Risk or blocker'),
      deep_dive: t('Deep dive'),
      unattributed_fact: t('Unattributed fact'),
    };
    return labels[kind] ?? kind;
  };
  const statusLabel = (status: ReviewStatus) => ({
    pending: t('Pending review'),
    accepted: t('Accepted'),
    rejected: t('Rejected'),
  }[status]);

  return (
    <div className="border-b border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-4 py-3">
      {hasPrebrief && (
        <details className="mb-3 rounded-lg border border-[var(--gold-border)] bg-[var(--gold-soft)] p-3">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-3 font-medium text-[var(--fg)]">
            <span>{t('Before this standup')} · {prebrief.series.join(', ')}</span>
            <ChevronDown size={16} />
          </summary>
          <div className="mt-3 grid gap-3 lg:grid-cols-3">
            <div>
              <h4 className="mb-2 text-sm font-semibold">{t('Open actions')}</h4>
              {prebrief.open_actions.length === 0 ? (
                <p className="text-sm text-[var(--fg3)]">{t('No accepted open actions')}</p>
              ) : prebrief.open_actions.map((action) => (
                <div key={action.id} className="mb-2 rounded-md bg-[var(--bg-canvas)] p-2 text-sm">
                  <div>{action.text}</div>
                  <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[var(--fg3)]">
                    <button type="button" className="text-[var(--gold)] hover:underline" onClick={() => router.push(sourceHref(action.source_meeting_id, action.source_start_ms))}>
                      {action.source_meeting_title}
                    </button>
                    {action.owner && <span>{action.owner}</span>}
                    {action.due_date && <span>{action.due_date}</span>}
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busyActionId === action.id}
                      onClick={() => void setActionStatus(action.id, 'done')}
                    >
                      <Check size={14} /> {t('Done')}
                    </Button>
                  </div>
                </div>
              ))}
            </div>
            <div>
              <h4 className="mb-2 text-sm font-semibold">{t('Recent accepted blockers')}</h4>
              {prebrief.recent_risks.length === 0 ? (
                <p className="text-sm text-[var(--fg3)]">{t('None recorded')}</p>
              ) : prebrief.recent_risks.map((fact) => (
                <button type="button" key={fact.record_id} className="mb-2 block w-full rounded-md bg-[var(--bg-canvas)] p-2 text-left text-sm hover:underline" onClick={() => router.push(sourceHref(fact.source_meeting_id, fact.source_start_ms))}>
                  {fact.text}
                </button>
              ))}
            </div>
            <div>
              <h4 className="mb-2 text-sm font-semibold">{t('Recent accepted decisions')}</h4>
              {prebrief.recent_decisions.length === 0 ? (
                <p className="text-sm text-[var(--fg3)]">{t('None recorded')}</p>
              ) : prebrief.recent_decisions.map((fact) => (
                <button type="button" key={fact.record_id} className="mb-2 block w-full rounded-md bg-[var(--bg-canvas)] p-2 text-left text-sm hover:underline" onClick={() => router.push(sourceHref(fact.source_meeting_id, fact.source_start_ms))}>
                  {fact.text}
                </button>
              ))}
            </div>
          </div>
        </details>
      )}

      {hasLiveState && (
        <details className="mb-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-3">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-3 font-medium text-[var(--fg)]">
            <span>{t('Live standup markers')}</span>
            <ChevronDown size={16} />
          </summary>
          <p className="mt-2 text-xs text-[var(--fg3)]">
            {t('Manual facilitation data is local context, not transcript evidence and not a score of any participant.')}
          </p>
          <div className="mt-3 flex flex-wrap gap-2 text-sm text-[var(--fg2)]">
            <span className="rounded bg-[var(--bg-elevated)] px-2 py-1">
              {t('Time-box')}: {liveState.targetMinutes}:00
            </span>
            <span className="rounded bg-[var(--bg-elevated)] px-2 py-1">
              {t('Updates covered')}: {liveState.completedUpdates}
            </span>
          </div>
          {liveState.markers.length > 0 && (
            <div className="mt-3 flex flex-wrap gap-2">
              {liveState.markers.map((marker) => (
                <a
                  key={marker.id}
                  href={`/meeting-details?id=${encodeURIComponent(meetingId)}&t=${marker.seconds}`}
                  className="rounded-full border border-[var(--gold-border)] px-2.5 py-1 text-xs text-[var(--gold)] hover:underline"
                >
                  {marker.kind === 'parking_lot' ? t('Parking lot') : t('Question')} · {Math.floor(marker.seconds / 60)}:{String(marker.seconds % 60).padStart(2, '0')}
                </a>
              ))}
            </div>
          )}
        </details>
      )}

      <details open={privateNotes.some((note) => note.status === 'open')} className="mb-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-3">
        <summary className="flex cursor-pointer list-none items-center justify-between gap-3 font-medium text-[var(--fg)]">
          <span>{t('Standup preparation and private notes')}</span>
          <ChevronDown size={16} />
        </summary>
        <p className="mt-2 text-xs text-[var(--fg3)]">
          {t('These notes stay local and are never treated as transcript evidence or sent to the summary model.')}
        </p>
        <div className="mt-3 flex flex-wrap gap-2">
          {(['planned_update', 'parking_lot', 'private_note'] as PrivateNoteKind[]).map((kind) => (
            <Button
              key={kind}
              size="sm"
              variant={newNoteKind === kind ? 'default' : 'outline'}
              onClick={() => setNewNoteKind(kind)}
            >
              {{
                planned_update: t('Planned update'),
                parking_lot: t('Parking lot'),
                private_note: t('Private scratchpad'),
              }[kind]}
            </Button>
          ))}
        </div>
        <div className="mt-2 flex gap-2">
          <Input
            disabled={noteBusy}
            maxLength={4000}
            placeholder={{
              planned_update: t('What do you plan to share?'),
              parking_lot: t('Topic to discuss after the status round'),
              private_note: t('A note visible only in this local meeting'),
            }[newNoteKind]}
            value={newNoteText}
            onChange={(event) => setNewNoteText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                void createPrivateNote();
              }
            }}
          />
          <Button disabled={noteBusy || !newNoteText.trim()} onClick={() => void createPrivateNote()}>
            {t('Add')}
          </Button>
        </div>
        {privateNotes.length > 0 && (
          <div className="mt-3 space-y-2">
            {privateNotes.map((note) => (
              <div key={note.id} className="flex items-start justify-between gap-3 rounded-md border border-[var(--border-subtle)] p-2 text-sm">
                <div>
                  <div className="mb-1 text-xs text-[var(--fg3)]">
                    {{
                      planned_update: t('Planned update'),
                      parking_lot: t('Parking lot'),
                      private_note: t('Private scratchpad'),
                    }[note.kind]}
                    {note.status === 'done' && ` · ${t('Done')}`}
                  </div>
                  <p className={note.status === 'done' ? 'text-[var(--fg3)] line-through' : 'text-[var(--fg)]'}>{note.text}</p>
                </div>
                <div className="flex shrink-0 gap-1">
                  {note.status === 'open' ? (
                    <Button size="sm" variant="ghost" disabled={noteBusy} onClick={() => void setPrivateNoteStatus(note.id, 'done')}>
                      <Check size={14} /> {t('Done')}
                    </Button>
                  ) : (
                    <Button size="sm" variant="ghost" disabled={noteBusy} onClick={() => void setPrivateNoteStatus(note.id, 'open')}>
                      {t('Reopen')}
                    </Button>
                  )}
                  <Button size="sm" variant="ghost" disabled={noteBusy} onClick={() => void setPrivateNoteStatus(note.id, 'archived')}>
                    <X size={14} />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </details>

      {records.length > 0 && (
        <details open={pendingCount > 0} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-3">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-3 font-medium text-[var(--fg)]">
            <span>{t('Review extracted standup records')} · {pendingCount} {t('pending review')}</span>
            <ChevronDown size={16} />
          </summary>
          <p className="mt-2 text-xs text-[var(--fg3)]">
            {t('Only accepted actions are carried into later standups. Evidence always stays attached to the original transcript.')}
          </p>
          <div className="mt-3 space-y-2">
            {records.map((record) => {
              const payload = effectivePayload(record);
              const isEditing = editingId === record.id;
              return (
                <div key={record.id} className="rounded-md border border-[var(--border-subtle)] p-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2 text-xs">
                      <span className="rounded bg-[var(--bg-elevated)] px-2 py-1 font-medium">{kindLabel(record.kind)}</span>
                      {payload.participant && <span>{payload.participant}</span>}
                      {payload.category && <span className="text-[var(--fg3)]">{payload.category}</span>}
                      <span className={record.review_status === 'accepted' ? 'text-[var(--success)]' : record.review_status === 'rejected' ? 'text-[var(--danger)]' : 'text-[var(--gold)]'}>
                        {statusLabel(record.review_status)}
                      </span>
                    </div>
                    <div className="flex flex-wrap gap-1">
                      {record.review_status !== 'accepted' && (
                        <Button size="sm" variant="outline" disabled={busyRecordId === record.id} onClick={() => void review(record, 'accepted')}>
                          <Check size={14} /> {t('Accept')}
                        </Button>
                      )}
                      <Button size="sm" variant="ghost" disabled={busyRecordId === record.id} onClick={() => beginEdit(record)}>
                        <Pencil size={14} /> {t('Edit')}
                      </Button>
                      {record.review_status !== 'rejected' ? (
                        <Button size="sm" variant="ghost" disabled={busyRecordId === record.id} onClick={() => void review(record, 'rejected')}>
                          <X size={14} /> {t('Reject')}
                        </Button>
                      ) : (
                        <Button size="sm" variant="ghost" disabled={busyRecordId === record.id} onClick={() => void review(record, 'pending')}>
                          <RefreshCw size={14} /> {t('Restore')}
                        </Button>
                      )}
                    </div>
                  </div>

                  {isEditing ? (
                    <div className="mt-3 space-y-2">
                      <Input value={draft.text} onChange={(event) => setDraft((value) => ({ ...value, text: event.target.value }))} />
                      {(record.kind === 'action' || record.kind === 'risk') && (
                        <Input placeholder={t('Owner, only when explicitly known')} value={draft.owner} onChange={(event) => setDraft((value) => ({ ...value, owner: event.target.value }))} />
                      )}
                      {record.kind === 'action' && (
                        <Input placeholder={t('Due date, only when explicitly known')} value={draft.dueDate} onChange={(event) => setDraft((value) => ({ ...value, dueDate: event.target.value }))} />
                      )}
                      <div className="flex gap-2">
                        <Button size="sm" disabled={busyRecordId === record.id || !draft.text.trim()} onClick={() => void review(record, 'accepted', true)}>
                          {t('Save and accept')}
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}>{t('Cancel')}</Button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <p className="mt-2 text-sm text-[var(--fg)]">{primaryText(record)}</p>
                      {(payload.owner || payload.due_date) && (
                        <p className="mt-1 text-xs text-[var(--fg3)]">
                          {payload.owner ?? t('unknown owner')}{payload.due_date ? ` · ${payload.due_date}` : ''}
                        </p>
                      )}
                    </>
                  )}

                  <div className="mt-2 flex flex-wrap gap-2 text-xs">
                    {evidence(record).map((item, index) => {
                      const href = evidenceHref(record.meeting_id, item.timestamp);
                      return href ? (
                        <button type="button" key={`${item.timestamp}-${index}`} onClick={() => router.push(href)} title={item.quote ?? undefined} className="text-[var(--gold)] hover:underline">
                          {item.timestamp}
                        </button>
                      ) : null;
                    })}
                    {record.kind === 'action' && record.action_item_id && (
                      record.action_status === 'done' ? (
                        <Button size="sm" variant="ghost" disabled={busyActionId === record.action_item_id} onClick={() => void setActionStatus(record.action_item_id!, 'open')}>
                          {t('Reopen action')}
                        </Button>
                      ) : (
                        <Button size="sm" variant="ghost" disabled={busyActionId === record.action_item_id} onClick={() => void setActionStatus(record.action_item_id!, 'done')}>
                          <Check size={14} /> {t('Mark done')}
                        </Button>
                      )
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </details>
      )}
    </div>
  );
}
