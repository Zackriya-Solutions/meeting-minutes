"use client";

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Check, ChevronDown, Pencil, RefreshCw, X } from '@/components/memento/LucideCompat';

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
  const [loading, setLoading] = useState(true);
  const [busyRecordId, setBusyRecordId] = useState<number | null>(null);
  const [busyActionId, setBusyActionId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [draft, setDraft] = useState<Draft>({ text: '', owner: '', dueDate: '' });

  const refresh = useCallback(async () => {
    try {
      const [nextRecords, nextPrebrief] = await Promise.all([
        invoke<StandupRecordRow[]>('list_standup_records', { meetingId }),
        invoke<StandupPrebrief>('get_standup_prebrief', { meetingId }),
      ]);
      setRecords(nextRecords);
      setPrebrief(nextPrebrief);
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
      setEditingId((current) => (current === record.id ? null : current));
      await refresh();
    } catch (error) {
      console.error('Failed to review standup record:', error);
      const detail = error instanceof Error ? error.message : String(error);
      toast.error(detail
        ? `${t('Failed to save standup review')}: ${detail}`
        : t('Failed to save standup review'));
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

  const hasPrebrief = prebrief.series.length > 0;
  if (loading || (!hasPrebrief && records.length === 0)) return null;

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
