"use client";

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import { Check, ChevronDown, RefreshCw, X } from '@/components/memento/LucideCompat';

type ReviewStatus = 'pending' | 'accepted' | 'rejected';

interface OneOnOneConfig {
  meetingId: string;
  pairId?: number | null;
  participantA?: string | null;
  participantARole?: string | null;
  participantB?: string | null;
  participantBRole?: string | null;
  sharedAgenda: string[];
  targetMinutes: number;
  facilitationEnabled: boolean;
  occurredAt?: string | null;
  occurredAtConfirmed: boolean;
}

interface OneOnOnePrivacy {
  cloudProcessingAllowed: boolean;
  indexingAllowed: boolean;
}

interface OneOnOnePrivateNote {
  id: number;
  participant_slot: 'participant_a' | 'participant_b';
  note_kind: 'agenda_draft' | 'scratchpad';
  content: string;
  shared_to_agenda: boolean;
}

interface OneOnOnePrebrief {
  ready: boolean;
  reason?: string | null;
  previousMeeting?: { meetingId: string; title: string; occurredAt: string } | null;
  openCommitments: OneOnOneCommitment[];
  acceptedCarry: Array<{ sourceMeetingId: string; sourceTitle: string; sourceOccurredAt: string; kind: string; payload: Record<string, any> }>;
  changesSincePrevious: Array<{ state: string; kind: string; payload: Record<string, any>; sourceMeetingId: string; comparisonMeetingId: string }>;
}

interface RecurringSuggestion {
  canonicalTopic: string;
  occurrences: number;
  sourceRecordIds: number[];
  confirmationRequired: boolean;
}

interface OneOnOneRecord {
  id: number;
  meeting_id: string;
  kind: string;
  payload: Record<string, any>;
  reviewed_payload?: Record<string, any> | null;
  review_status: ReviewStatus;
  carry_status: 'open' | 'closed';
}

interface OneOnOneCommitment {
  id: number;
  task: string;
  owner?: string | null;
  due_date?: string | null;
  status: 'open' | 'done' | 'cancelled' | 'superseded';
}

const splitLines = (value: string) => value.split('\n').map((item) => item.trim()).filter(Boolean);

const isDueBefore = (dueDate?: string | null, occurredAt?: string | null) => Boolean(
  dueDate && occurredAt && /^\d{4}-\d{2}-\d{2}$/.test(dueDate) && dueDate < occurredAt.slice(0, 10),
);

function effectivePayload(record: OneOnOneRecord): Record<string, any> {
  return record.reviewed_payload ?? record.payload;
}

function primaryField(kind: string): string {
  if (kind === 'previous_follow_up') return 'commitment';
  if (kind === 'challenge_support') return 'challenge';
  if (kind === 'feedback') return 'observation';
  if (kind === 'growth' || kind === 'open_topic') return 'topic';
  if (kind === 'commitment') return 'task';
  return 'text';
}

function evidenceHref(meetingId: string, timestamp: string): string | null {
  const match = /^\[(\d+):(\d{2})\]$/.exec(timestamp.trim());
  if (!match) return null;
  return `/meeting-details?id=${encodeURIComponent(meetingId)}&t=${Number(match[1]) * 60 + Number(match[2])}`;
}

export function OneOnOneWorkflowPanel({
  meetingId,
  summaryStatus,
  oneOnOneSelected,
}: {
  meetingId: string;
  summaryStatus: string;
  oneOnOneSelected: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(true);
  const [busy, setBusy] = useState(false);
  const [config, setConfig] = useState<OneOnOneConfig | null>(null);
  const [records, setRecords] = useState<OneOnOneRecord[]>([]);
  const [commitments, setCommitments] = useState<OneOnOneCommitment[]>([]);
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const [agenda, setAgenda] = useState('');
  const [privacy, setPrivacy] = useState<OneOnOnePrivacy>({ cloudProcessingAllowed: false, indexingAllowed: false });
  const [noteSlot, setNoteSlot] = useState<'participant_a' | 'participant_b'>('participant_a');
  const [privateNotes, setPrivateNotes] = useState<OneOnOnePrivateNote[]>([]);
  const [privateDraft, setPrivateDraft] = useState('');
  const [privateKind, setPrivateKind] = useState<'agenda_draft' | 'scratchpad'>('agenda_draft');
  const [prebrief, setPrebrief] = useState<OneOnOnePrebrief | null>(null);
  const [recurring, setRecurring] = useState<RecurringSuggestion[]>([]);
  const [markerNote, setMarkerNote] = useState('');
  const [sessionStartedAt] = useState(() => Date.now());
  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  useEffect(() => {
    if (!oneOnOneSelected) return;
    const update = () => setElapsedSeconds(Math.max(0, Math.round((Date.now() - sessionStartedAt) / 1000)));
    update();
    const timer = window.setInterval(update, 1_000);
    return () => window.clearInterval(timer);
  }, [oneOnOneSelected, sessionStartedAt]);

  const load = useCallback(async () => {
    if (!oneOnOneSelected) return;
    try {
      const [nextConfig, nextRecords, nextCommitments, nextPrivacy, nextNotes, nextPrebrief, nextRecurring] = await Promise.all([
        invoke<OneOnOneConfig>('get_one_on_one_config', { meetingId }),
        invoke<OneOnOneRecord[]>('list_one_on_one_records', { meetingId }),
        invoke<OneOnOneCommitment[]>('list_one_on_one_commitments', { meetingId }),
        invoke<OneOnOnePrivacy>('get_one_on_one_privacy', { meetingId }),
        invoke<OneOnOnePrivateNote[]>('list_one_on_one_private_notes', { meetingId, participantSlot: noteSlot }),
        invoke<OneOnOnePrebrief>('get_one_on_one_prebrief', { meetingId }),
        invoke<RecurringSuggestion[]>('list_one_on_one_recurring_suggestions', { meetingId }),
      ]);
      setConfig(nextConfig);
      setAgenda((nextConfig.sharedAgenda ?? []).join('\n'));
      setRecords(nextRecords);
      setCommitments(nextCommitments);
      setPrivacy(nextPrivacy);
      setPrivateNotes(nextNotes);
      setPrebrief(nextPrebrief);
      setRecurring(nextRecurring);
    } catch (error) {
      console.error('Failed to load One-on-One Memory workflow:', error);
    }
  }, [meetingId, noteSlot, oneOnOneSelected]);

  useEffect(() => { void load(); }, [load, summaryStatus]);

  const pendingCount = useMemo(
    () => records.filter((record) => record.review_status === 'pending').length,
    [records],
  );

  if (!oneOnOneSelected) return null;

  const saveConfig = async () => {
    if (!config) return;
    setBusy(true);
    try {
      const saved = await invoke<OneOnOneConfig>('save_one_on_one_config', {
        config: { ...config, sharedAgenda: splitLines(agenda) },
      });
      setConfig(saved);
      setAgenda(saved.sharedAgenda.join('\n'));
      toast.success(t('One-on-one preparation saved'));
    } catch (error) {
      toast.error(t('Failed to save one-on-one preparation'), { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const review = async (record: OneOnOneRecord, status: ReviewStatus) => {
    const payload = effectivePayload(record);
    const field = primaryField(record.kind);
    const draft = drafts[record.id];
    const changed = draft !== undefined && draft !== (payload[field] ?? '');
    try {
      await invoke('review_one_on_one_record', {
        input: {
          recordId: record.id,
          status,
          editedPayload: status === 'accepted' && changed
            ? { ...payload, [field]: draft.trim() }
            : null,
        },
      });
      await load();
    } catch (error) {
      toast.error(t('Failed to review one-on-one record'), { description: String(error) });
    }
  };

  const updateCommitment = async (commitmentId: number, status: OneOnOneCommitment['status']) => {
    try {
      await invoke('set_one_on_one_commitment_status', { input: { commitmentId, status } });
      await load();
    } catch (error) {
      toast.error(t('Failed to update one-on-one commitment'), { description: String(error) });
    }
  };

  const updateTopic = async (recordId: number, status: 'open' | 'closed') => {
    try {
      await invoke('set_one_on_one_topic_status', { input: { recordId, status } });
      await load();
    } catch (error) {
      toast.error(t('Failed to update open topic'), { description: String(error) });
    }
  };

  const savePrivacy = async (next: OneOnOnePrivacy) => {
    try {
      const saved = await invoke<OneOnOnePrivacy>('save_one_on_one_privacy', { meetingId, privacy: next });
      setPrivacy(saved);
      toast.success(t('One-on-one privacy saved'));
    } catch (error) {
      toast.error(t('Failed to save one-on-one privacy'), { description: String(error) });
    }
  };

  const addPrivateNote = async () => {
    const content = privateDraft.trim();
    if (!content) return;
    try {
      await invoke('save_one_on_one_private_note', {
        input: { meetingId, participantSlot: noteSlot, noteKind: privateKind, content },
      });
      setPrivateDraft('');
      await load();
    } catch (error) {
      toast.error(t('Failed to save private note'), { description: String(error) });
    }
  };

  const sharePrivateNote = async (noteId: number) => {
    try {
      const saved = await invoke<OneOnOneConfig>('share_one_on_one_private_note_to_agenda', { meetingId, noteId });
      setConfig(saved);
      setAgenda(saved.sharedAgenda.join('\n'));
      await load();
    } catch (error) {
      toast.error(t('Failed to share agenda topic'), { description: String(error) });
    }
  };

  const deletePrivateNote = async (noteId: number) => {
    try {
      await invoke('delete_one_on_one_private_note', { meetingId, noteId });
      await load();
    } catch (error) {
      toast.error(t('Failed to delete private note'), { description: String(error) });
    }
  };

  const addMarker = async (markerKind: string) => {
    try {
      await invoke('add_one_on_one_live_marker', {
        input: {
          meetingId,
          markerKind,
          elapsedSeconds: Math.max(0, Math.round((Date.now() - sessionStartedAt) / 1000)),
          note: markerNote.trim() || null,
        },
      });
      setMarkerNote('');
      toast.success(t('Conversation marker saved'));
    } catch (error) {
      toast.error(t('Failed to save conversation marker'), { description: String(error) });
    }
  };

  const confirmRecurring = async (suggestion: RecurringSuggestion) => {
    try {
      await invoke('confirm_one_on_one_recurring_topic', {
        input: { meetingId, canonicalTopic: suggestion.canonicalTopic, sourceRecordIds: suggestion.sourceRecordIds },
      });
      toast.success(t('Recurring topic confirmed'));
    } catch (error) {
      toast.error(t('Failed to confirm recurring topic'), { description: String(error) });
    }
  };

  const exportAccepted = async () => {
    try {
      const value = await invoke<Record<string, any>>('export_one_on_one_accepted_memory', { meetingId });
      const blob = new Blob([`${JSON.stringify(value, null, 2)}\n`], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `one-on-one-${meetingId}-accepted.json`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      toast.error(t('Failed to export accepted one-on-one memory'), { description: String(error) });
    }
  };

  const deleteSeriesMemory = async () => {
    if (!config?.pairId || !window.confirm(t('Delete the derived memory for every meeting in this confirmed pair? Transcripts remain, but summaries, review records, private notes, markers, commitments, and search embeddings are removed.'))) return;
    try {
      await invoke('delete_one_on_one_series_memory', { meetingId, confirmPairId: config.pairId });
      toast.success(t('One-on-one series memory deleted'));
      window.location.reload();
    } catch (error) {
      toast.error(t('Failed to delete one-on-one series memory'), { description: String(error) });
    }
  };

  return (
    <section className="mx-1 mb-4 rounded-xl border border-[var(--gold-border)] bg-[var(--bg-elevated)]/70">
      <button
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
        onClick={() => setOpen((value) => !value)}
      >
        <span>
          <span className="block font-semibold text-[var(--fg1)]">{t('One-on-One Memory')}</span>
          <span className="text-xs text-[var(--fg3)]">
            {pendingCount} {t('records need review')} · {t('sensitive by default')}
          </span>
        </span>
        <ChevronDown size={18} className={open ? 'rotate-180 transition-transform' : 'transition-transform'} />
      </button>

      {open && config && (
        <div className="space-y-5 border-t border-[var(--border-subtle)] p-4">
          <div className="space-y-3">
            <div>
              <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Participants and shared agenda')}</h3>
              <p className="mt-1 text-xs text-[var(--fg3)]">
                {t('Roles are user-provided context, never inferred from the transcript.')}
              </p>
            </div>
            <div className="grid gap-2 md:grid-cols-2">
              <Input
                value={config.participantA ?? ''}
                onChange={(event) => setConfig({ ...config, participantA: event.target.value })}
                placeholder={t('Participant A')}
              />
              <Input
                value={config.participantARole ?? ''}
                onChange={(event) => setConfig({ ...config, participantARole: event.target.value })}
                placeholder={t('Participant A role')}
              />
              <Input
                value={config.participantB ?? ''}
                onChange={(event) => setConfig({ ...config, participantB: event.target.value })}
                placeholder={t('Participant B')}
              />
              <Input
                value={config.participantBRole ?? ''}
                onChange={(event) => setConfig({ ...config, participantBRole: event.target.value })}
                placeholder={t('Participant B role')}
              />
            </div>
            <Textarea
              value={agenda}
              onChange={(event) => setAgenda(event.target.value)}
              placeholder={t('Shared agenda, one topic per line')}
              rows={4}
            />
            <div className="grid gap-2 md:grid-cols-2">
              <label className="text-xs text-[var(--fg3)]">
                {t('Confirmed meeting date')}
                <Input
                  className="mt-1"
                  type="date"
                  value={(config.occurredAt ?? '').slice(0, 10)}
                  onChange={(event) => setConfig({ ...config, occurredAt: event.target.value, occurredAtConfirmed: false })}
                />
              </label>
              <label className="flex items-center justify-between gap-3 rounded-md border border-[var(--border-subtle)] px-3 text-sm">
                <span>{t('I confirm this date')}</span>
                <Switch
                  checked={config.occurredAtConfirmed}
                  onCheckedChange={(checked) => setConfig({ ...config, occurredAtConfirmed: checked })}
                />
              </label>
            </div>
            <div className="flex items-center gap-2">
              <Input
                className="w-28"
                type="number"
                min={10}
                max={180}
                value={config.targetMinutes}
                onChange={(event) => setConfig({ ...config, targetMinutes: Number(event.target.value) })}
                aria-label={t('Target minutes')}
              />
              <Button onClick={saveConfig} disabled={busy} size="sm">
                {busy ? <RefreshCw className="mr-2 h-4 w-4 animate-spin" /> : null}
                {t('Save preparation')}
              </Button>
              <label className="ml-auto flex items-center gap-2 text-sm">
                <span>{t('Facilitation mode')}</span>
                <Switch
                  checked={config.facilitationEnabled}
                  onCheckedChange={(checked) => setConfig({ ...config, facilitationEnabled: checked })}
                />
              </label>
            </div>
          </div>

          <div className="space-y-3 rounded-lg border border-[var(--border-subtle)] p-3">
            <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Sensitive memory controls')}</h3>
            <p className="text-xs text-[var(--fg3)]">{t('Both options require explicit consent. Private notes are never exported or sent to the summary model.')}</p>
            {([
              ['cloudProcessingAllowed', 'Allow cloud processing for this memory'],
              ['indexingAllowed', 'Include in search and Memento chat'],
            ] as const).map(([key, label]) => (
              <label key={key} className="flex items-center justify-between gap-3 text-sm">
                <span>{t(label)}</span>
                <Switch checked={privacy[key]} onCheckedChange={(checked) => void savePrivacy({ ...privacy, [key]: checked })} />
              </label>
            ))}
          </div>

          <div className="space-y-3 rounded-lg border border-[var(--border-subtle)] p-3">
            <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Series pre-brief')}</h3>
            {!prebrief?.ready ? (
              <p className="text-sm text-[var(--fg3)]">{t(prebrief?.reason ?? 'Confirm both participants and the meeting date to enable series memory')}</p>
            ) : (
              <>
                {prebrief.previousMeeting && (
                  <a className="text-sm text-[var(--gold)] hover:underline" href={`/meeting-details?id=${encodeURIComponent(prebrief.previousMeeting.meetingId)}`}>
                    {t('Previous one-on-one')}: {prebrief.previousMeeting.title} · {prebrief.previousMeeting.occurredAt}
                  </a>
                )}
                <p className="text-xs text-[var(--fg3)]">{t('Only accepted records from confirmed earlier meetings are carried forward.')}</p>
                {prebrief.openCommitments.map((item) => (
                  <div key={`carry-${item.id}`} className="rounded-md bg-[var(--bg-base)] p-2 text-sm">
                    {item.task} · {item.owner ?? t('unknown owner')} · {item.due_date ?? t('due date not stated')}
                    {isDueBefore(item.due_date, config.occurredAt) && (
                      <p className="mt-1 text-xs text-[var(--fg3)]">{t('Worth checking in on this still-open commitment; its stated date has passed.')}</p>
                    )}
                  </div>
                ))}
                {prebrief.acceptedCarry.map((item, index) => (
                  <a key={`${item.sourceMeetingId}-${index}`} href={`/meeting-details?id=${encodeURIComponent(item.sourceMeetingId)}`} className="block text-xs text-[var(--fg2)] hover:underline">
                    {item.sourceOccurredAt} · {t(item.kind)} · {String(item.payload.topic ?? item.payload.observation ?? '')}
                  </a>
                ))}
                {prebrief.changesSincePrevious.length > 0 && (
                  <div className="space-y-1 border-t border-[var(--border-subtle)] pt-2">
                    <p className="text-xs font-semibold text-[var(--fg2)]">{t('Changed since the previous one-on-one')}</p>
                    {prebrief.changesSincePrevious.map((item, index) => (
                      <div key={`${item.sourceMeetingId}-${item.state}-${index}`} className="text-xs text-[var(--fg2)]">
                        {t(item.state)} · {String(item.payload.task ?? item.payload.topic ?? item.payload.text ?? item.payload.observation ?? '')}{' · '}
                        <a className="text-[var(--gold)] hover:underline" href={`/meeting-details?id=${encodeURIComponent(item.sourceMeetingId)}`}>{t('source')}</a>{' ↔ '}
                        <a className="text-[var(--gold)] hover:underline" href={`/meeting-details?id=${encodeURIComponent(item.comparisonMeetingId)}`}>{t('comparison')}</a>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
            <Button size="sm" variant="outline" onClick={() => void exportAccepted()}>{t('Export accepted memory')}</Button>
            {config.pairId && <Button size="sm" variant="destructive" onClick={() => void deleteSeriesMemory()}>{t('Delete pair memory')}</Button>}
          </div>

          <div className="space-y-3 rounded-lg border border-[var(--border-subtle)] p-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Private notes')}</h3>
                <p className="text-xs text-[var(--fg3)]">{t('Stored separately and never used as transcript evidence.')}</p>
              </div>
              <select
                className="rounded-md border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 py-1 text-sm"
                value={noteSlot}
                onChange={(event) => setNoteSlot(event.target.value as 'participant_a' | 'participant_b')}
              >
                <option value="participant_a">{config.participantA || t('Participant A')}</option>
                <option value="participant_b">{config.participantB || t('Participant B')}</option>
              </select>
            </div>
            <div className="flex gap-2">
              <select
                className="rounded-md border border-[var(--border-subtle)] bg-[var(--bg-base)] px-2 text-sm"
                value={privateKind}
                onChange={(event) => setPrivateKind(event.target.value as 'agenda_draft' | 'scratchpad')}
              >
                <option value="agenda_draft">{t('Agenda draft')}</option>
                <option value="scratchpad">{t('Scratchpad')}</option>
              </select>
              <Input value={privateDraft} onChange={(event) => setPrivateDraft(event.target.value)} placeholder={t('Private note')} />
              <Button size="sm" onClick={() => void addPrivateNote()}>{t('Add')}</Button>
            </div>
            {privateNotes.map((note) => (
              <div key={note.id} className="flex items-start justify-between gap-3 rounded-md bg-[var(--bg-base)] p-2 text-sm">
                <span>{note.content}</span>
                <div className="flex shrink-0 gap-1">
                  {note.note_kind === 'agenda_draft' && !note.shared_to_agenda && (
                    <Button size="sm" variant="ghost" onClick={() => void sharePrivateNote(note.id)}>{t('Share to agenda')}</Button>
                  )}
                  <Button size="sm" variant="ghost" onClick={() => void deletePrivateNote(note.id)}>{t('Delete')}</Button>
                </div>
              </div>
            ))}
          </div>

          {config.facilitationEnabled && (
            <div className="space-y-3 rounded-lg border border-[var(--gold-border)] p-3">
              <div>
                <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Live facilitation')}</h3>
                <p className="text-xs text-[var(--fg3)]">
                  {String(Math.floor(elapsedSeconds / 60)).padStart(2, '0')}:{String(elapsedSeconds % 60).padStart(2, '0')} / {config.targetMinutes}:00 · {t('Leave time for mutual feedback and next steps. No tone or speaking-time analysis is performed.')}
                </p>
              </div>
              <Input value={markerNote} onChange={(event) => setMarkerNote(event.target.value)} placeholder={t('Optional marker note')} />
              <div className="flex flex-wrap gap-2">
                {['feedback', 'support', 'growth', 'follow_up', 'return_later', 'deep_dive'].map((kind) => (
                  <Button key={kind} size="sm" variant="outline" onClick={() => void addMarker(kind)}>{t(kind)}</Button>
                ))}
              </div>
            </div>
          )}

          <div className="space-y-2">
            <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Evidence review')}</h3>
            {records.length === 0 && (
              <p className="text-sm text-[var(--fg3)]">{t('Generate One-on-One Memory to create reviewable records.')}</p>
            )}
            {records.map((record) => {
              const payload = effectivePayload(record);
              const field = primaryField(record.kind);
              const evidence = Array.isArray(payload.evidence) ? payload.evidence : [];
              return (
                <div key={record.id} className="rounded-lg border border-[var(--border-subtle)] p-3">
                  <div className="mb-2 flex items-center justify-between gap-2 text-xs text-[var(--fg3)]">
                    <span>{t(record.kind)}</span>
                    <span>{t(record.review_status)}</span>
                  </div>
                  <Textarea
                    rows={2}
                    value={drafts[record.id] ?? String(payload[field] ?? '')}
                    onChange={(event) => setDrafts((current) => ({ ...current, [record.id]: event.target.value }))}
                  />
                  <div className="mt-2 flex flex-wrap gap-2">
                    {evidence.map((item: any, index: number) => {
                      const href = evidenceHref(meetingId, String(item.timestamp ?? ''));
                      return href ? (
                        <a key={`${record.id}-${index}`} href={href} className="text-xs text-[var(--gold)] hover:underline">
                          {item.timestamp} · {item.quote}
                        </a>
                      ) : null;
                    })}
                  </div>
                  <div className="mt-3 flex gap-2">
                    <Button size="sm" variant="outline" onClick={() => review(record, 'accepted')}>
                      <Check className="mr-1 h-4 w-4" />{t('Accept')}
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => review(record, 'rejected')}>
                      <X className="mr-1 h-4 w-4" />{t('Reject')}
                    </Button>
                    {record.kind === 'open_topic' && record.review_status === 'accepted' && (
                      <Button size="sm" variant="ghost" onClick={() => updateTopic(record.id, record.carry_status === 'open' ? 'closed' : 'open')}>
                        {t(record.carry_status === 'open' ? 'Close for next one-on-one' : 'Reopen for next one-on-one')}
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>

          {commitments.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Accepted commitments')}</h3>
              {commitments.map((commitment) => (
                <div key={commitment.id} className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-subtle)] p-3">
                  <div className="min-w-0">
                    <p className="text-sm text-[var(--fg1)]">{commitment.task}</p>
                    <p className="text-xs text-[var(--fg3)]">
                      {commitment.owner ?? t('unknown owner')} · {commitment.due_date ?? t('due date not stated')} · {t(commitment.status)}
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-1">
                    <Button size="sm" variant="ghost" onClick={() => updateCommitment(commitment.id, 'done')}>{t('Done')}</Button>
                    <Button size="sm" variant="ghost" onClick={() => updateCommitment(commitment.id, 'cancelled')}>{t('Cancel')}</Button>
                    <Button size="sm" variant="ghost" onClick={() => updateCommitment(commitment.id, 'superseded')}>{t('Supersede')}</Button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {recurring.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Possible recurring topics')}</h3>
              <p className="text-xs text-[var(--fg3)]">{t('Memento only suggests exact repeats in accepted records. Nothing is linked until you confirm it.')}</p>
              {recurring.map((suggestion) => (
                <div key={suggestion.canonicalTopic} className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-subtle)] p-3">
                  <span className="text-sm">{suggestion.canonicalTopic} · {suggestion.occurrences}</span>
                  <Button size="sm" variant="outline" onClick={() => void confirmRecurring(suggestion)}>{t('Confirm recurring topic')}</Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
