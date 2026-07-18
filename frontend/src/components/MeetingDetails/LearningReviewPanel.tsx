'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { Play } from '@/components/memento/LucideCompat';

interface MeetingTypeSuggestion {
  id: number;
  suggested_type: string;
  confidence: number;
  reasons: string[];
}

interface CollectionSuggestion {
  id: number;
  suggested_name: string;
  suggestion_kind: string;
  confidence: number;
  reasons: string[];
}

interface IdentityCandidate {
  speaker_id: number;
  display_name: string;
  voice_score: number;
  combined_score: number;
  confidence_band: string;
}

interface IdentityReviewItem {
  cluster_id: number;
  local_cluster_id: number;
  operational_speaker_id?: number | null;
  operational_display_name?: string | null;
  speech_duration_ms: number;
  speech_quality?: number | null;
  policy_result: string;
  candidates: IdentityCandidate[];
  latest_assertion?: string | null;
  samples: IdentityReviewSample[];
}

export interface IdentityReviewSample {
  transcript_id: string;
  start_seconds: number;
  end_seconds?: number | null;
  text: string;
}

interface AdvancedLearningProfile {
  speaker_id: number;
  enabled: boolean;
  support_meetings: number;
}

interface SpeakerProfileVersion {
  version: number;
  is_active: boolean;
  sample_count: number;
  created_at: string;
}

interface TermRow {
  id: number;
  canonical: string;
  aliases: string[];
  status: string;
  confidence: number;
  support_count: number;
}

interface ReconciliationRow {
  id: number;
  suggestion_kind: string;
  confidence: number;
  status: string;
  evidence: Record<string, unknown>;
  previous_value: Record<string, unknown>;
  proposed_value: Record<string, unknown>;
}

interface MeetingWindowRow {
  id: number;
  start_offset_ms: number;
  end_offset_ms?: number | null;
  suggested_start_ms?: number | null;
  suggested_end_ms?: number | null;
  confirmed_start_ms?: number | null;
  confirmed_end_ms?: number | null;
  boundary_source: string;
  confidence?: number | null;
  review_status: string;
}

const meetingTypes = [
  'general', 'standup', 'planning', 'project_sync', 'one_on_one',
  'interview', 'client_sync', 'technical_deep_dive', 'uncertain',
];

const meetingTypeLabelKeys: Record<string, string> = {
  general: 'General meeting',
  standup: 'Standup',
  planning: 'Planning',
  project_sync: 'Project sync',
  one_on_one: 'One-on-one',
  interview: 'Interview',
  client_sync: 'Client sync',
  technical_deep_dive: 'Technical deep dive',
  uncertain: 'Uncertain',
};

function meetingTypeLabel(type: string, t: (value: string) => string) {
  return t(meetingTypeLabelKeys[type] ?? type);
}

function percent(value: number | null | undefined) {
  return `${Math.round((value ?? 0) * 100)}%`;
}

function clock(totalSeconds: number) {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
    : `${minutes}:${String(remainder).padStart(2, '0')}`;
}

function localizedSpeakerLabel(label: string | null | undefined, t: (value: string) => string) {
  const automatic = label?.match(/^Speaker (\d+)$/);
  return automatic ? `${t('Speaker')} ${automatic[1]}` : label;
}

export function LearningReviewPanel({
  meetingId,
  onChanged,
  onPlayIdentitySample,
}: {
  meetingId: string;
  onChanged?: () => Promise<void> | void;
  onPlayIdentitySample?: (sample: IdentityReviewSample) => void;
}) {
  const t = useT();
  const [classification, setClassification] = useState<MeetingTypeSuggestion | null>(null);
  const [selectedType, setSelectedType] = useState('uncertain');
  const [collections, setCollections] = useState<CollectionSuggestion[]>([]);
  const [identities, setIdentities] = useState<IdentityReviewItem[]>([]);
  const [terms, setTerms] = useState<TermRow[]>([]);
  const [reconciliation, setReconciliation] = useState<ReconciliationRow[]>([]);
  const [windows, setWindows] = useState<MeetingWindowRow[]>([]);
  const [windowEdits, setWindowEdits] = useState<Record<number, { start: number; end: number }>>({});
  const [allowLearning, setAllowLearning] = useState<Record<number, boolean>>({});
  const [identityNames, setIdentityNames] = useState<Record<number, string>>({});
  const [advancedProfiles, setAdvancedProfiles] = useState<Record<number, AdvancedLearningProfile>>({});
  const [profileVersions, setProfileVersions] = useState<Record<number, SpeakerProfileVersion[]>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoadError(null);
    try {
      const [type, collectionRows, identityRows, termRows, reconciliationRows, meetingWindows] = await Promise.all([
        invoke<MeetingTypeSuggestion | null>('get_meeting_classification_review', { meetingId }),
        invoke<CollectionSuggestion[]>('get_collection_classification_review', { meetingId }),
        invoke<IdentityReviewItem[]>('get_identity_review', { meetingId }),
        invoke<TermRow[]>('list_terminology_memory', { status: 'pending', meetingId }),
        invoke<ReconciliationRow[]>('list_reconciliation_suggestions', { meetingId }),
        invoke<MeetingWindowRow[]>('list_meeting_windows', { meetingId }),
      ]);
      setClassification(type);
      setSelectedType(type?.suggested_type ?? 'uncertain');
      setCollections(collectionRows);
      setIdentities(identityRows);
      const confirmedSpeakerIds = [...new Set(identityRows
        .filter((item) => item.latest_assertion?.startsWith('positive:trusted'))
        .map((item) => item.operational_speaker_id)
        .filter((id): id is number => id != null))];
      const controls = await Promise.all(confirmedSpeakerIds.map(async (speakerId) => {
        const [advanced, versions] = await Promise.all([
          invoke<AdvancedLearningProfile>('get_speaker_advanced_learning', { speakerId }),
          invoke<SpeakerProfileVersion[]>('list_speaker_profile_versions', { speakerId }),
        ]);
        return { speakerId, advanced, versions };
      }));
      setAdvancedProfiles(Object.fromEntries(controls.map(({ speakerId, advanced }) => [speakerId, advanced])));
      setProfileVersions(Object.fromEntries(controls.map(({ speakerId, versions }) => [speakerId, versions])));
      setTerms(termRows.slice(0, 10));
      setReconciliation(reconciliationRows);
      setWindows(meetingWindows);
      setWindowEdits(Object.fromEntries(meetingWindows.map((window) => [
        window.id,
        {
          start: (window.confirmed_start_ms ?? window.suggested_start_ms ?? window.start_offset_ms) / 1000,
          end: (window.confirmed_end_ms ?? window.suggested_end_ms ?? window.end_offset_ms ?? window.start_offset_ms) / 1000,
        },
      ])));
    } catch (error) {
      console.error('Failed to load learning review:', error);
      setLoadError(String(error));
    }
  }, [meetingId]);

  useEffect(() => { void load(); }, [load]);

  const pendingIdentities = useMemo(
    () => identities.filter((item) => !item.latest_assertion?.startsWith('positive:trusted')),
    [identities],
  );
  const confirmedIdentities = useMemo(() => {
    const bySpeaker = new Map<number, IdentityReviewItem>();
    identities
      .filter((item) => item.latest_assertion?.startsWith('positive:trusted') && item.operational_speaker_id != null)
      .forEach((item) => bySpeaker.set(item.operational_speaker_id!, item));
    return [...bySpeaker.values()];
  }, [identities]);
  const pendingWindows = windows.filter((window) => window.review_status === 'pending');
  const pendingReconciliation = reconciliation.filter((row) => row.status === 'pending');
  const totalPending = (classification ? 1 : 0) + collections.length + pendingIdentities.length + terms.length + pendingReconciliation.length + pendingWindows.length;

  const reviewType = async (status: 'accepted' | 'rejected') => {
    if (!classification) return;
    setBusy(`type-${status}`);
    try {
      await invoke('review_meeting_classification', {
        input: { suggestionId: classification.id, status, correctedType: selectedType },
      });
      setClassification(null);
      toast.success(t('Meeting classification reviewed'));
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to review meeting classification')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const reviewIdentity = async (
    item: IdentityReviewItem,
    decision: 'confirm' | 'reject' | 'unknown',
    speakerId?: number,
    displayName?: string,
  ) => {
    setBusy(`identity-${item.cluster_id}`);
    try {
      await invoke('review_speaker_identity', {
        input: {
          clusterId: item.cluster_id,
          decision,
          speakerId: decision === 'confirm' ? speakerId ?? null : null,
          displayName: decision === 'confirm' ? displayName?.trim() || null : null,
          rejectedSpeakerId: decision === 'reject' ? speakerId ?? null : null,
          allowLearning: decision === 'confirm' && !!allowLearning[item.cluster_id],
          scope: 'cluster',
        },
      });
      toast.success(t('Speaker identity reviewed'));
      setIdentityNames((current) => {
        const next = { ...current };
        delete next[item.cluster_id];
        return next;
      });
      await load();
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to review speaker identity')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const reviewCollection = async (suggestion: CollectionSuggestion, status: 'accepted' | 'rejected') => {
    setBusy(`collection-${suggestion.id}`);
    try {
      await invoke('review_collection_classification', {
        input: { suggestionId: suggestion.id, status },
      });
      toast.success(t('Collection classification reviewed'));
      await load();
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to review collection classification')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const reviewTerm = async (term: TermRow, status: 'confirmed' | 'rejected') => {
    setBusy(`term-${term.id}`);
    try {
      await invoke('review_terminology_memory', { input: { termId: term.id, status } });
      toast.success(t('Terminology memory reviewed'));
      await load();
    } catch (error) {
      toast.error(`${t('Failed to review terminology')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const reviewBackfill = async (row: ReconciliationRow, decision: 'accepted' | 'rejected') => {
    setBusy(`reconcile-${row.id}`);
    try {
      await invoke('review_reconciliation_suggestion', {
        input: { suggestionId: row.id, decision },
      });
      toast.success(t('Historical correction reviewed'));
      await load();
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to review historical correction')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const rollbackBackfill = async (row: ReconciliationRow) => {
    if (!window.confirm(t('Restore the previous value for this historical correction?'))) return;
    setBusy(`reconcile-${row.id}`);
    try {
      await invoke('rollback_reconciliation_suggestion', { suggestionId: row.id });
      toast.success(t('Historical correction rolled back'));
      await load();
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to roll back historical correction')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const setAdvancedLearning = async (speakerId: number, enabled: boolean) => {
    setBusy(`speaker-${speakerId}`);
    try {
      const profile = await invoke<AdvancedLearningProfile>('set_speaker_advanced_learning', {
        input: { speakerId, enabled },
      });
      setAdvancedProfiles((current) => ({ ...current, [speakerId]: profile }));
      toast.success(t('Advanced speaker memory updated'));
    } catch (error) {
      toast.error(`${t('Advanced memory requires voice-learning consent and three reviewed meetings')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const rollbackProfile = async (speakerId: number, version: number) => {
    if (!window.confirm(`${t('Roll back the voice profile to version')} ${version}?`)) return;
    setBusy(`speaker-${speakerId}`);
    try {
      await invoke('rollback_speaker_profile', { speakerId, version });
      toast.success(t('Voice profile rolled back'));
      await load();
    } catch (error) {
      toast.error(`${t('Failed to roll back voice profile')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const purgeSpeakerMemory = async (speakerId: number) => {
    if (!window.confirm(t('Delete all learned voice, context, language, and dynamics data for this speaker? Transcripts remain, but identity labels are detached.'))) return;
    setBusy(`speaker-${speakerId}`);
    try {
      await invoke('purge_speaker_learning_data', { speakerId });
      toast.success(t('Speaker learning data deleted'));
      await load();
      await onChanged?.();
    } catch (error) {
      toast.error(`${t('Failed to delete speaker learning data')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const reviewWindow = async (window: MeetingWindowRow, status: 'accepted' | 'rejected') => {
    const edit = windowEdits[window.id];
    setBusy(`window-${window.id}`);
    try {
      await invoke('review_meeting_window', {
        input: {
          windowId: window.id,
          status,
          startOffsetMs: edit ? Math.round(edit.start * 1000) : null,
          endOffsetMs: edit ? Math.round(edit.end * 1000) : null,
        },
      });
      toast.success(t('Meeting boundary reviewed'));
      await load();
    } catch (error) {
      toast.error(`${t('Failed to review meeting boundary')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  const splitWindow = async (window: MeetingWindowRow) => {
    const edit = windowEdits[window.id];
    if (!edit || edit.end <= edit.start) return;
    setBusy(`window-${window.id}`);
    try {
      await invoke('split_meeting_window', {
        input: { windowId: window.id, splitOffsetMs: Math.round(((edit.start + edit.end) / 2) * 1000) },
      });
      await load();
    } catch (error) {
      toast.error(`${t('Failed to split meeting window')}: ${String(error)}`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <details className="mx-4 mt-4 rounded-xl border border-[var(--gold-border)] bg-[var(--gold-soft)] px-4 py-3" open={totalPending > 0}>
      <summary className="cursor-pointer select-none text-sm font-semibold text-[var(--fg1)]">
        {t('Memento learning review')} · {totalPending} {t('items pending review')}
      </summary>
      {loadError && <p className="mt-3 text-xs text-[var(--danger)]">{loadError}</p>}
      <div className="mt-3 grid gap-3 xl:grid-cols-2">
        {classification && (
          <section className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
            <div className="text-xs font-semibold text-[var(--fg1)]">{t('Meeting type')}</div>
            <p className="mt-1 text-xs text-[var(--fg3)]">
              {t('Suggested')} {meetingTypeLabel(classification.suggested_type, t)} · {percent(classification.confidence)}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <select
                value={selectedType}
                onChange={(event) => setSelectedType(event.target.value)}
                className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-2 py-1 text-xs"
              >
                {meetingTypes.map((type) => (
                  <option key={type} value={type}>{meetingTypeLabel(type, t)}</option>
                ))}
              </select>
              <button disabled={busy != null} onClick={() => void reviewType('accepted')} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Accept')}</button>
              <button disabled={busy != null} onClick={() => void reviewType('rejected')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Reject')}</button>
            </div>
          </section>
        )}

        {collections.map((suggestion) => (
          <section key={suggestion.id} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
            <div className="text-xs font-semibold text-[var(--fg1)]">{t('Collection')}</div>
            <p className="mt-1 text-xs text-[var(--fg2)]">
              {t('Suggested')} <strong>{suggestion.suggested_name}</strong> · {percent(suggestion.confidence)}
            </p>
            <div className="mt-2 flex gap-2">
              <button disabled={busy != null} onClick={() => void reviewCollection(suggestion, 'accepted')} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Add to collection')}</button>
              <button disabled={busy != null} onClick={() => void reviewCollection(suggestion, 'rejected')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Reject')}</button>
            </div>
          </section>
        ))}

        {pendingIdentities.map((item) => {
          const top = item.candidates[0];
          const speakerLabel = localizedSpeakerLabel(item.operational_display_name, t);
          return (
            <section key={item.cluster_id} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
              <div className="text-xs font-semibold text-[var(--fg1)]">
                {speakerLabel ?? `${t('Unassigned voice')} ${item.local_cluster_id + 1}`} · {Math.round(item.speech_duration_ms / 1000)}s
              </div>
              <p className="mt-1 text-[11px] text-[var(--fg3)]">
                {t('Voice detection group')} {item.local_cluster_id + 1} ·{' '}
                {speakerLabel
                  ? t('linked to this speaker label in the transcript')
                  : t('not linked to a speaker in the transcript yet')}
              </p>
              <p className="mt-1 text-xs text-[var(--fg3)]">
                {top
                  ? `${top.display_name} · ${t('voice match')} ${percent(top.voice_score)} · ${t(top.confidence_band)}`
                  : t('No reliable identity candidate')}
              </p>
              <div className="mt-2 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-2">
                <p className="mb-1.5 text-[11px] text-[var(--fg2)]">
                  {t('Listen to excerpts before assigning a name')}
                </p>
                {item.samples.length > 0 ? (
                  <div className="grid gap-1">
                    {item.samples.map((sample) => (
                      <button
                        key={sample.transcript_id}
                        type="button"
                        disabled={!onPlayIdentitySample}
                        onClick={() => onPlayIdentitySample?.(sample)}
                        title={t('Play voice excerpt and open it in the transcript')}
                        className="flex min-w-0 items-center gap-2 rounded px-2 py-1.5 text-left text-xs text-[var(--fg2)] transition-colors hover:bg-[var(--gold-soft)] disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        <Play className="h-3.5 w-3.5 shrink-0 text-[var(--gold)]" />
                        <span className="mm-numeric shrink-0 text-[var(--gold)]">{clock(sample.start_seconds)}</span>
                        <span className="min-w-0 truncate">{sample.text}</span>
                      </button>
                    ))}
                  </div>
                ) : (
                  <p className="text-[11px] text-[var(--fg3)]">{t('No timed voice excerpts are available')}</p>
                )}
              </div>
              {!top && (
                <input
                  value={identityNames[item.cluster_id] ?? ''}
                  onChange={(event) => setIdentityNames((current) => ({
                    ...current,
                    [item.cluster_id]: event.target.value,
                  }))}
                  placeholder={t('Enter speaker name')}
                  className="mt-2 w-full rounded-md border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-2 py-1.5 text-xs"
                />
              )}
              <label className="mt-2 flex items-center gap-2 text-[11px] text-[var(--fg2)]">
                <input
                  type="checkbox"
                  checked={!!allowLearning[item.cluster_id]}
                  onChange={(event) => setAllowLearning((current) => ({ ...current, [item.cluster_id]: event.target.checked }))}
                />
                {t('Use this confirmed sample to improve future voice recognition')}
              </label>
              <div className="mt-2 flex flex-wrap gap-2">
                {top && <button disabled={busy != null} onClick={() => void reviewIdentity(item, 'confirm', top.speaker_id)} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Confirm')} {top.display_name}</button>}
                {top && <button disabled={busy != null} onClick={() => void reviewIdentity(item, 'reject', top.speaker_id)} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Not this person')}</button>}
                {!top && (
                  <button
                    disabled={busy != null || !identityNames[item.cluster_id]?.trim()}
                    onClick={() => void reviewIdentity(
                      item,
                      'confirm',
                      undefined,
                      identityNames[item.cluster_id],
                    )}
                    className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)] disabled:opacity-50"
                  >
                    {t('Confirm speaker name')}
                  </button>
                )}
                <button disabled={busy != null} onClick={() => void reviewIdentity(item, 'unknown')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Unknown')}</button>
              </div>
            </section>
          );
        })}

        {confirmedIdentities.map((item) => {
          const speakerId = item.operational_speaker_id!;
          const candidate = item.candidates.find((value) => value.speaker_id === speakerId);
          const advanced = advancedProfiles[speakerId];
          const versions = profileVersions[speakerId] ?? [];
          const confirmedLabel = localizedSpeakerLabel(
            candidate?.display_name ?? item.operational_display_name,
            t,
          ) ?? t('Confirmed speaker');
          return (
            <section key={`speaker-memory-${speakerId}`} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
              <div className="text-xs font-semibold text-[var(--fg1)]">
                {t('Speaker memory')} · {confirmedLabel}
              </div>
              <label className="mt-2 flex items-center gap-2 text-[11px] text-[var(--fg2)]">
                <input
                  type="checkbox"
                  checked={advanced?.enabled ?? false}
                  disabled={busy != null}
                  onChange={(event) => void setAdvancedLearning(speakerId, event.target.checked)}
                />
                {t('Build opt-in shadow language and conversation-dynamics profiles')}
              </label>
              {advanced?.enabled && <p className="mt-1 text-[11px] text-[var(--fg3)]">{advanced.support_meetings} {t('reviewed meetings')}</p>}
              <div className="mt-2 flex flex-wrap gap-2">
                {versions.filter((version) => !version.is_active).slice(0, 3).map((version) => (
                  <button key={version.version} disabled={busy != null} onClick={() => void rollbackProfile(speakerId, version.version)} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">
                    {t('Rollback')} v{version.version}
                  </button>
                ))}
                <button disabled={busy != null} onClick={() => void purgeSpeakerMemory(speakerId)} className="rounded-md border border-[var(--danger)] px-2 py-1 text-xs text-[var(--danger)]">
                  {t('Delete learned data')}
                </button>
              </div>
            </section>
          );
        })}

        {terms.map((term) => (
          <section key={term.id} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
            <div className="text-xs font-semibold text-[var(--fg1)]">{t('Terminology')}</div>
            <p className="mt-1 text-xs text-[var(--fg2)]">
              {term.aliases.join(', ') || '—'} → <strong>{term.canonical}</strong> · {term.support_count}×
            </p>
            <div className="mt-2 flex gap-2">
              <button disabled={busy != null} onClick={() => void reviewTerm(term, 'confirmed')} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Confirm')}</button>
              <button disabled={busy != null} onClick={() => void reviewTerm(term, 'rejected')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Reject')}</button>
            </div>
          </section>
        ))}

        {reconciliation.map((row) => (
          <section key={row.id} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
            <div className="text-xs font-semibold text-[var(--fg1)]">{t('Historical correction')}</div>
            <p className="mt-1 break-words text-xs text-[var(--fg3)]">
              {row.suggestion_kind} · {percent(row.confidence)} · {row.status}
            </p>
            <div className="mt-2 flex gap-2">
              {row.status === 'pending' && <button disabled={busy != null} onClick={() => void reviewBackfill(row, 'accepted')} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Apply')}</button>}
              {row.status === 'pending' && <button disabled={busy != null} onClick={() => void reviewBackfill(row, 'rejected')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Reject')}</button>}
              {row.status === 'applied' && <button disabled={busy != null} onClick={() => void rollbackBackfill(row)} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Rollback')}</button>}
            </div>
          </section>
        ))}

        {pendingWindows.map((window) => {
          const edit = windowEdits[window.id] ?? { start: 0, end: 0 };
          return (
            <section key={window.id} className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-3">
              <div className="text-xs font-semibold text-[var(--fg1)]">{t('Meeting boundary')} · {window.boundary_source}</div>
              <p className="mt-1 text-xs text-[var(--fg3)]">
                {t('Review the detected start and end. A split creates two reviewable windows over the same local recording.')}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                <label>{t('Start, sec')} <input type="number" min={0} step={1} value={Math.round(edit.start)} onChange={(event) => setWindowEdits((current) => ({ ...current, [window.id]: { ...edit, start: Number(event.target.value) } }))} className="ml-1 w-20 rounded border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-2 py-1" /></label>
                <label>{t('End, sec')} <input type="number" min={0} step={1} value={Math.round(edit.end)} onChange={(event) => setWindowEdits((current) => ({ ...current, [window.id]: { ...edit, end: Number(event.target.value) } }))} className="ml-1 w-20 rounded border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-2 py-1" /></label>
              </div>
              <div className="mt-2 flex flex-wrap gap-2">
                <button disabled={busy != null} onClick={() => void reviewWindow(window, 'accepted')} className="rounded-md bg-[var(--gold)] px-2 py-1 text-xs text-[var(--fg-inverse)]">{t('Accept')}</button>
                <button disabled={busy != null} onClick={() => void splitWindow(window)} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Split at midpoint')}</button>
                <button disabled={busy != null} onClick={() => void reviewWindow(window, 'rejected')} className="rounded-md border border-[var(--border-strong)] px-2 py-1 text-xs">{t('Reject')}</button>
              </div>
            </section>
          );
        })}

        {!loadError && totalPending === 0 && (
          <p className="text-xs text-[var(--fg3)]">{t('Nothing needs review for this meeting.')}</p>
        )}
      </div>
    </details>
  );
}
