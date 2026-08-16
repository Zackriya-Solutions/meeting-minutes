"use client";

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Tag, Loader2, Check, X } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useT } from '@/lib/i18n';
import { localizeSpeakerLabel, SpeakerInfo } from '@/types';

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
  /** The word is a name we recognise — these lead the list. */
  is_recognized_name?: boolean;
}

const EVIDENCE_LABELS: Record<string, string> = {
  self_introduction: 'Self introduction',
  explicit_introduction: 'Explicit introduction',
  direct_address: 'Direct address',
  direct_address_unassigned: 'Name mentioned in an address',
  meeting_title: 'Name mentioned in the meeting title',
};

/**
 * The review surface for transcript-derived name evidence, controlled by its caller.
 *
 * Split out of the button below so the "⋯" menu can open it: the redesigned meeting screen
 * dropped the transcript toolbar, which was this flow's only entry point, and the candidates
 * kept accumulating in the database with nobody able to look at them.
 */
export function SpeakerNameCandidatesDialog({
  meetingId,
  open,
  onOpenChange,
  onApplied,
}: {
  meetingId?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onApplied?: () => Promise<void> | void;
}) {
  const t = useT();
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [speakers, setSpeakers] = useState<SpeakerInfo[]>([]);
  const [targets, setTargets] = useState<Record<number, number | ''>>({});
  const [rename, setRename] = useState<Record<number, boolean>>({});
  const [showRest, setShowRest] = useState(false);

  // A transcript's address slot holds far more particles and verbs than names, so the
  // full list buries the one or two candidates worth acting on. Lead with the words we
  // recognise as names; the rest stays one click away rather than gone, because the
  // lexicon is finite and somebody's name will be missing from it.
  const recognized = candidates.filter((candidate) => candidate.is_recognized_name);
  const rest = candidates.filter((candidate) => !candidate.is_recognized_name);
  const visible = showRest || recognized.length === 0 ? candidates : recognized;

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
      setShowRest(false);
      setSpeakers(nextSpeakers);
      setTargets(Object.fromEntries(nextCandidates.map((candidate) => [
        candidate.id,
        candidate.proposed_speaker_id ?? '',
      ])));
    } catch (error) {
      console.error('Failed to scan speaker names:', error);
      toast.error(t('Failed to find speaker name candidates'), { description: String(error) });
    } finally {
      setLoading(false);
    }
  };

  const handleOpen = (value: boolean) => {
    onOpenChange(value);
  };

  useEffect(() => {
    if (open) void load();
    // Re-scanning on every render would hammer the transcript; the open edge is the trigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, meetingId]);

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
          setAsDisplayName: status === 'accepted' && (rename[candidate.id] ?? true),
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
      <Dialog open={open} onOpenChange={handleOpen}>
        <DialogContent className="max-h-[80vh] overflow-y-auto sm:max-w-[640px]">
          <DialogHeader>
            <DialogTitle>{t('Speaker name candidates')}</DialogTitle>
            <DialogDescription>
              {t('Names are local suggestions, never automatic identity changes. Check the evidence and choose the matching speaker yourself.')}
            </DialogDescription>
          </DialogHeader>

          {loading ? (
            <div className="flex items-center justify-center gap-2 py-8 text-sm text-muted-foreground">
              <Loader2 className="animate-spin" size={18} /> {t('Scanning transcript...')}
            </div>
          ) : candidates.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">{t('No safe name candidates found')}</p>
          ) : (
            <div className="space-y-3">
              {visible.map((candidate) => {
                const seconds = Math.floor((candidate.evidence_start_ms ?? 0) / 1000);
                return (
                  <div key={candidate.id} className="rounded-lg border border-border p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <strong>{candidate.candidate_text}</strong>
                        <span className="ml-2 text-xs text-muted-foreground">
                          {t(EVIDENCE_LABELS[candidate.evidence_kind] ?? 'Name evidence')} · {Math.round(candidate.confidence * 100)}%
                          {candidate.occurrence_count > 1 ? ` · ×${candidate.occurrence_count}` : ''}
                        </span>
                      </div>
                      <button
                        type="button"
                        className="text-xs text-primary hover:underline"
                        onClick={() => {
                          onOpenChange(false);
                          router.push(`/meeting-details?id=${encodeURIComponent(meetingId)}&t=${seconds}`);
                        }}
                      >
                        {t('Open evidence')}
                      </button>
                    </div>
                    {candidate.evidence_quote && (
                      <p className="mt-2 line-clamp-3 text-sm text-muted-foreground">{candidate.evidence_quote}</p>
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
                          <option key={speaker.id} value={speaker.id}>
                            {localizeSpeakerLabel(speaker.display_name, t)}
                          </option>
                        ))}
                      </select>
                      <label className="flex items-center gap-2 text-xs text-muted-foreground">
                        <input
                          type="checkbox"
                          checked={rename[candidate.id] ?? true}
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
              {rest.length > 0 && !showRest && (
                <button
                  type="button"
                  className="w-full rounded-lg border border-dashed border-border py-2 text-sm text-muted-foreground hover:text-foreground"
                  onClick={() => setShowRest(true)}
                >
                  {t('Show weaker evidence')} ({rest.length})
                </button>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}

/** The transcript-toolbar entry point. Kept for the legacy meeting layout. */
export function SpeakerNameCandidatesButton({
  meetingId,
  onApplied,
}: {
  meetingId?: string;
  onApplied?: () => Promise<void> | void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);

  if (!meetingId) return null;

  return (
    <>
      <Button
        size="sm"
        variant="outline"
        onClick={() => setOpen(true)}
        title={t('Find names used in the transcript')}
      >
        <Tag size={18} />
        <span className="hidden lg:inline">{t('Names')}</span>
      </Button>
      <SpeakerNameCandidatesDialog
        meetingId={meetingId}
        open={open}
        onOpenChange={setOpen}
        onApplied={onApplied}
      />
    </>
  );
}
