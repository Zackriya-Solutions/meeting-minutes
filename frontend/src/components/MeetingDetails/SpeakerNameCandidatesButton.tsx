"use client";

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Tag, Loader2, Check, X } from '@/components/memento/LucideCompat';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useT } from '@/lib/i18n';
import { SpeakerInfo } from '@/types';

interface Candidate {
  id: number;
  meeting_id: string;
  proposed_speaker_id?: number | null;
  candidate_text?: string | null;
  evidence_kind: string;
  evidence_quote?: string | null;
  evidence_start_ms?: number | null;
  confidence: number;
  occurrence_count: number;
}

export function SpeakerNameCandidatesButton({
  meetingId,
  onApplied,
}: {
  meetingId?: string;
  onApplied?: () => Promise<void> | void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [speakers, setSpeakers] = useState<SpeakerInfo[]>([]);
  const [targets, setTargets] = useState<Record<number, number | ''>>({});
  const [rename, setRename] = useState<Record<number, boolean>>({});

  const load = async () => {
    if (!meetingId) return;
    setLoading(true);
    try {
      await invoke('scan_speaker_name_candidates', { meetingId });
      const [nextCandidates, nextSpeakers] = await Promise.all([
        invoke<Candidate[]>('list_speaker_name_candidates', { meetingId }),
        invoke<SpeakerInfo[]>('get_meeting_speakers', { meetingId }),
      ]);
      setCandidates(nextCandidates);
      setSpeakers(nextSpeakers);
      setTargets(Object.fromEntries(nextCandidates.map((candidate) => [
        candidate.id,
        candidate.proposed_speaker_id ?? '',
      ])));
    } catch (error) {
      console.error('Failed to scan speaker names:', error);
      toast.error(t('Failed to find speaker name candidates'));
    } finally {
      setLoading(false);
    }
  };

  const handleOpen = (value: boolean) => {
    setOpen(value);
    if (value) void load();
  };

  const review = async (candidate: Candidate, status: 'accepted' | 'rejected') => {
    const speakerId = targets[candidate.id];
    if (status === 'accepted' && speakerId === '') {
      toast.error(t('Choose a speaker before accepting the name'));
      return;
    }
    setBusyId(candidate.id);
    try {
      await invoke('review_speaker_name_candidate', {
        input: {
          candidateId: candidate.id,
          status,
          speakerId: speakerId === '' ? null : speakerId,
          setAsDisplayName: status === 'accepted' && Boolean(rename[candidate.id]),
        },
      });
      setCandidates((current) => current.filter((item) => item.id !== candidate.id));
      if (status === 'accepted') await onApplied?.();
    } catch (error) {
      console.error('Failed to review speaker name:', error);
      toast.error(typeof error === 'string' ? error : t('Failed to save speaker name'));
    } finally {
      setBusyId(null);
    }
  };

  if (!meetingId) return null;

  return (
    <>
      <Button size="sm" variant="outline" onClick={() => handleOpen(true)} title={t('Find names used in the transcript')}>
        <Tag size={18} />
        <span className="hidden lg:inline">{t('Names')}</span>
      </Button>
      <Dialog open={open} onOpenChange={handleOpen}>
        <DialogContent className="max-h-[80vh] overflow-y-auto sm:max-w-[640px]">
          <DialogHeader>
            <DialogTitle>{t('Speaker name candidates')}</DialogTitle>
            <DialogDescription>
              {t('Names are local suggestions, never automatic identity changes. Check the evidence and choose the matching speaker yourself.')}
            </DialogDescription>
          </DialogHeader>

          {loading ? (
            <div className="flex items-center justify-center gap-2 py-8 text-sm text-[var(--fg3)]">
              <Loader2 className="animate-spin" size={18} /> {t('Scanning transcript...')}
            </div>
          ) : candidates.length === 0 ? (
            <p className="py-6 text-center text-sm text-[var(--fg3)]">{t('No safe name candidates found')}</p>
          ) : (
            <div className="space-y-3">
              {candidates.map((candidate) => {
                const seconds = Math.floor((candidate.evidence_start_ms ?? 0) / 1000);
                return (
                  <div key={candidate.id} className="rounded-lg border border-[var(--border-subtle)] p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <strong>{candidate.candidate_text}</strong>
                        <span className="ml-2 text-xs text-[var(--fg3)]">
                          {t(candidate.evidence_kind)} · {Math.round(candidate.confidence * 100)}%
                          {candidate.occurrence_count > 1 ? ` · ×${candidate.occurrence_count}` : ''}
                        </span>
                      </div>
                      <a
                        className="text-xs text-[var(--gold)] hover:underline"
                        href={`/meeting-details?id=${encodeURIComponent(meetingId)}&t=${seconds}`}
                      >
                        {t('Open evidence')}
                      </a>
                    </div>
                    {candidate.evidence_quote && (
                      <p className="mt-2 line-clamp-3 text-sm text-[var(--fg2)]">{candidate.evidence_quote}</p>
                    )}
                    <div className="mt-3 flex flex-wrap items-center gap-2">
                      <select
                        className="mm-field min-w-[180px] px-2 py-1.5 text-sm"
                        value={targets[candidate.id] ?? ''}
                        onChange={(event) => setTargets((current) => ({
                          ...current,
                          [candidate.id]: event.target.value ? Number(event.target.value) : '',
                        }))}
                      >
                        <option value="">{t('Choose speaker')}</option>
                        {speakers.map((speaker) => (
                          <option key={speaker.id} value={speaker.id}>{speaker.display_name}</option>
                        ))}
                      </select>
                      <label className="flex items-center gap-2 text-xs text-[var(--fg2)]">
                        <input
                          type="checkbox"
                          checked={Boolean(rename[candidate.id])}
                          onChange={(event) => setRename((current) => ({
                            ...current,
                            [candidate.id]: event.target.checked,
                          }))}
                        />
                        {t('Use as display name')}
                      </label>
                      <div className="ml-auto flex gap-1">
                        <Button size="sm" variant="outline" disabled={busyId === candidate.id} onClick={() => void review(candidate, 'accepted')}>
                          <Check size={14} /> {t('Accept')}
                        </Button>
                        <Button size="sm" variant="ghost" disabled={busyId === candidate.id} onClick={() => void review(candidate, 'rejected')}>
                          <X size={14} /> {t('Reject')}
                        </Button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
