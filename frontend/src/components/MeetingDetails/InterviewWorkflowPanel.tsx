"use client";

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Check, ChevronDown, Download, RefreshCw, Shield, X } from '@/components/deslop-icons';

type ReviewStatus = 'pending' | 'accepted' | 'rejected';

interface InterviewConfig {
  meetingId: string;
  candidateName?: string | null;
  roleTitle?: string | null;
  interviewStage?: string | null;
  interviewerRoles: string[];
  competencies: string[];
  successCriteria?: string | null;
  questionPlan: string[];
  glossary: string[];
  targetMinutes: number;
  candidateQuestionsMinutes: number;
}

interface InterviewPrivacy {
  meetingId: string;
  cloudProcessingAllowed: boolean;
  indexingAllowed: boolean;
  retentionDays?: number | null;
  retentionExpiresAt?: string | null;
  candidateExportAllowed: boolean;
}

interface InterviewRecord {
  id: number;
  meeting_id: string;
  kind: string;
  payload: Record<string, any>;
  reviewed_payload?: Record<string, any> | null;
  review_status: ReviewStatus;
}

interface Debrief {
  id: number;
  reviewerName: string;
  strengths: string;
  concerns: string;
  openQuestions: string;
  recommendation: string;
}

interface Handoff {
  track_id: number;
  current_stage_order?: number | null;
  previously_covered_competencies: string[];
  open_questions: Array<{
    source_meeting_title: string;
    question: string;
    reason?: string | null;
    competency?: string | null;
  }>;
}

const splitLines = (value: string) => value.split('\n').map((item) => item.trim()).filter(Boolean);
const joinLines = (value: string[] | undefined) => (value ?? []).join('\n');

function effectivePayload(record: InterviewRecord): Record<string, any> {
  return record.reviewed_payload ?? record.payload;
}

function primaryField(kind: string): string {
  if (kind === 'conversation_block') return 'topic';
  if (kind === 'question_answer') return 'question';
  if (kind === 'evidence') return 'observation';
  if (kind === 'case_exercise') return 'prompt';
  if (kind === 'next_step') return 'action';
  return 'question';
}

function evidenceHref(meetingId: string, timestamp: string): string | null {
  const match = /^\[(\d+):(\d{2})\]$/.exec(timestamp.trim());
  if (!match) return null;
  return `/meeting-details?id=${encodeURIComponent(meetingId)}&t=${Number(match[1]) * 60 + Number(match[2])}`;
}

function downloadText(filename: string, markdown: string) {
  const blob = new Blob([markdown], { type: 'text/markdown;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function InterviewWorkflowPanel({
  meetingId,
  summaryStatus,
  interviewSelected,
}: {
  meetingId: string;
  summaryStatus: string;
  interviewSelected: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(true);
  const [busy, setBusy] = useState(false);
  const [config, setConfig] = useState<InterviewConfig | null>(null);
  const [privacy, setPrivacy] = useState<InterviewPrivacy | null>(null);
  const [records, setRecords] = useState<InterviewRecord[]>([]);
  const [debriefs, setDebriefs] = useState<Debrief[]>([]);
  const [handoff, setHandoff] = useState<Handoff | null>(null);
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const [reviewerName, setReviewerName] = useState('');
  const [strengths, setStrengths] = useState('');
  const [concerns, setConcerns] = useState('');
  const [openQuestions, setOpenQuestions] = useState('');
  const [recommendation, setRecommendation] = useState('pending');
  const [trackCandidate, setTrackCandidate] = useState('');
  const [trackRole, setTrackRole] = useState('');
  const [trackId, setTrackId] = useState('');
  const [stageOrder, setStageOrder] = useState('1');
  const [stageName, setStageName] = useState('');

  const load = useCallback(async () => {
    if (!interviewSelected) return;
    try {
      const [nextConfig, nextPrivacy, nextRecords, nextDebriefs, nextHandoff] = await Promise.all([
        invoke<InterviewConfig>('get_interview_config', { meetingId }),
        invoke<InterviewPrivacy>('get_interview_privacy', { meetingId }),
        invoke<InterviewRecord[]>('list_interview_records', { meetingId }),
        invoke<Debrief[]>('list_interview_debriefs', { meetingId }),
        invoke<Handoff>('get_interview_handoff', { meetingId }),
      ]);
      setConfig(nextConfig);
      setPrivacy(nextPrivacy);
      setRecords(nextRecords);
      setDebriefs(nextDebriefs);
      setHandoff(nextHandoff);
      if (nextConfig.candidateName) setTrackCandidate(nextConfig.candidateName);
      if (nextConfig.roleTitle) setTrackRole(nextConfig.roleTitle);
    } catch (error) {
      console.error('Failed to load Interview Memory workflow:', error);
    }
  }, [interviewSelected, meetingId]);

  useEffect(() => { void load(); }, [load, summaryStatus]);

  const pendingCount = useMemo(
    () => records.filter((record) => record.review_status === 'pending').length,
    [records],
  );

  if (!interviewSelected) return null;

  const saveConfig = async () => {
    if (!config) return;
    setBusy(true);
    try {
      setConfig(await invoke<InterviewConfig>('save_interview_config', { config }));
      toast.success(t('Interview preparation saved'));
    } catch (error) {
      toast.error(t('Failed to save interview preparation'), { description: String(error) });
    } finally { setBusy(false); }
  };

  const savePrivacy = async (next: InterviewPrivacy) => {
    setPrivacy(next);
    try {
      setPrivacy(await invoke<InterviewPrivacy>('save_interview_privacy', { privacy: next }));
      toast.success(t('Interview privacy saved'));
    } catch (error) {
      toast.error(t('Failed to save interview privacy'), { description: String(error) });
      void load();
    }
  };

  const review = async (record: InterviewRecord, status: ReviewStatus) => {
    const payload = effectivePayload(record);
    const field = primaryField(record.kind);
    const changed = drafts[record.id] !== undefined && drafts[record.id] !== (payload[field] ?? '');
    try {
      await invoke('review_interview_record', {
        input: {
          recordId: record.id,
          status,
          editedPayload: status === 'accepted' && changed
            ? { ...payload, [field]: drafts[record.id].trim() }
            : null,
        },
      });
      await load();
    } catch (error) {
      toast.error(t('Failed to review interview evidence'), { description: String(error) });
    }
  };

  const saveDebrief = async () => {
    try {
      await invoke('save_interview_debrief', {
        input: { meetingId, reviewerName, strengths, concerns, openQuestions, recommendation },
      });
      setStrengths(''); setConcerns(''); setOpenQuestions('');
      await load();
      toast.success(t('Independent debrief saved'));
    } catch (error) {
      toast.error(t('Failed to save debrief'), { description: String(error) });
    }
  };

  const createTrack = async () => {
    try {
      const track = await invoke<{ id: number }>('create_interview_track', {
        input: { candidateName: trackCandidate, roleTitle: trackRole },
      });
      setTrackId(String(track.id));
      toast.success(t('Interview process created'));
    } catch (error) {
      toast.error(t('Failed to create interview process'), { description: String(error) });
    }
  };

  const assignStage = async () => {
    try {
      await invoke('assign_interview_stage', {
        input: { trackId: Number(trackId), meetingId, stageOrder: Number(stageOrder), stageName: stageName || null },
      });
      await load();
      toast.success(t('Interview stage linked'));
    } catch (error) {
      toast.error(t('Failed to link interview stage'), { description: String(error) });
    }
  };

  const exportMemory = async (audience: 'internal' | 'candidate') => {
    try {
      const result = await invoke<{ filename: string; markdown: string }>('export_interview_memory', { meetingId, audience });
      downloadText(result.filename, result.markdown);
    } catch (error) {
      toast.error(t('Failed to export interview memory'), { description: String(error) });
    }
  };

  return (
    <section className="mx-1 mb-4 rounded-xl border border-primary/40 bg-muted/70">
      <Button variant="ghost" className="h-auto w-full justify-between gap-3 px-4 py-3 text-left" onClick={() => setOpen((value) => !value)}>
        <span>
          <span className="block font-semibold text-foreground">{t('Interview Memory')}</span>
          <span className="text-xs text-muted-foreground">{pendingCount} {t('records need review')} · {t('sensitive by default')}</span>
        </span>
        <ChevronDown size={18} className={open ? 'rotate-180 transition-transform' : 'transition-transform'} />
      </Button>

      {open && config && privacy && (
        <div className="space-y-5 border-t border-border p-4">
          <div className="space-y-3">
            <h3 className="text-sm font-semibold">{t('Preparation and rubric')}</h3>
            <div className="grid gap-2 md:grid-cols-3">
              <Input value={config.candidateName ?? ''} placeholder={t('Candidate name')} onChange={(e) => setConfig({ ...config, candidateName: e.target.value })} />
              <Input value={config.roleTitle ?? ''} placeholder={t('Role')} onChange={(e) => setConfig({ ...config, roleTitle: e.target.value })} />
              <Input value={config.interviewStage ?? ''} placeholder={t('Interview stage')} onChange={(e) => setConfig({ ...config, interviewStage: e.target.value })} />
            </div>
            <Textarea value={joinLines(config.competencies)} placeholder={t('Competencies, one per line')} onChange={(e) => setConfig({ ...config, competencies: splitLines(e.target.value) })} />
            <Textarea value={config.successCriteria ?? ''} placeholder={t('30/60/90 day success criteria')} onChange={(e) => setConfig({ ...config, successCriteria: e.target.value })} />
            <Textarea value={joinLines(config.questionPlan)} placeholder={t('Question plan, one per line')} onChange={(e) => setConfig({ ...config, questionPlan: splitLines(e.target.value) })} />
            <Textarea value={joinLines(config.glossary)} placeholder={t('Role glossary for transcription, one term per line')} onChange={(e) => setConfig({ ...config, glossary: splitLines(e.target.value) })} />
            <div className="grid gap-2 sm:grid-cols-2">
              <Input type="number" min={10} max={240} value={config.targetMinutes} onChange={(e) => setConfig({ ...config, targetMinutes: Number(e.target.value) })} />
              <Input type="number" min={0} max={60} value={config.candidateQuestionsMinutes} onChange={(e) => setConfig({ ...config, candidateQuestionsMinutes: Number(e.target.value) })} />
            </div>
            <Button disabled={busy} onClick={() => void saveConfig()}>{busy ? <RefreshCw className="animate-spin" size={16} /> : <Check size={16} />}{t('Save preparation')}</Button>
          </div>

          <div className="space-y-3 rounded-lg border border-border p-3">
            <h3 className="flex items-center gap-2 text-sm font-semibold"><Shield size={16} />{t('Sensitive memory controls')}</h3>
            {([
              ['cloudProcessingAllowed', 'Allow cloud processing for this memory'],
              ['indexingAllowed', 'Include in search and Memento chat'],
              ['candidateExportAllowed', 'Allow candidate-safe export'],
            ] as const).map(([key, label]) => (
              <label key={key} className="flex items-center justify-between gap-3 text-sm">
                <span>{t(label)}</span>
                <Switch checked={privacy[key]} onCheckedChange={(checked) => void savePrivacy({ ...privacy, [key]: checked })} />
              </label>
            ))}
            <label className="block text-sm">
              <span>{t('Retention in days')}</span>
              <Input className="mt-1" type="number" min={1} max={3650} value={privacy.retentionDays ?? ''} placeholder={t('Keep indefinitely')} onChange={(e) => setPrivacy({ ...privacy, retentionDays: e.target.value ? Number(e.target.value) : null })} onBlur={() => void savePrivacy(privacy)} />
            </label>
          </div>

          <div className="space-y-3">
            <div className="flex items-center justify-between"><h3 className="text-sm font-semibold">{t('Evidence review')}</h3><Button size="sm" variant="ghost" onClick={() => void load()}><RefreshCw size={15} /></Button></div>
            {records.length === 0 ? <p className="text-sm text-muted-foreground">{t('Generate Interview Memory to create reviewable records.')}</p> : records.map((record) => {
              const payload = effectivePayload(record);
              const field = primaryField(record.kind);
              const refs = Array.isArray(payload.evidence) ? payload.evidence : [];
              return (
                <div key={record.id} className="space-y-2 rounded-lg border border-border p-3">
                  <div className="flex items-center justify-between gap-2"><span className="text-xs uppercase text-muted-foreground">{record.kind.replaceAll('_', ' ')}</span><span className="text-xs">{t(record.review_status)}</span></div>
                  <Input value={drafts[record.id] ?? payload[field] ?? ''} onChange={(e) => setDrafts({ ...drafts, [record.id]: e.target.value })} />
                  <div className="flex flex-wrap gap-2 text-xs">{refs.map((ref: any) => { const href = evidenceHref(meetingId, ref.timestamp); return href ? <a className="text-primary underline" key={ref.timestamp + ref.quote} href={href}>{ref.timestamp} {ref.quote}</a> : null; })}</div>
                  <div className="flex gap-2"><Button size="sm" onClick={() => void review(record, 'accepted')}><Check size={14} />{t('Accept')}</Button><Button size="sm" variant="outline" onClick={() => void review(record, 'rejected')}><X size={14} />{t('Reject')}</Button></div>
                </div>
              );
            })}
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-semibold">{t('Independent interviewer debrief')}</h3>
            <Input value={reviewerName} placeholder={t('Reviewer name')} onChange={(e) => setReviewerName(e.target.value)} />
            <Textarea value={strengths} placeholder={t('Job-relevant strengths with evidence')} onChange={(e) => setStrengths(e.target.value)} />
            <Textarea value={concerns} placeholder={t('Concerns and missing evidence')} onChange={(e) => setConcerns(e.target.value)} />
            <Textarea value={openQuestions} placeholder={t('Questions for the next stage')} onChange={(e) => setOpenQuestions(e.target.value)} />
            <Select value={recommendation} onValueChange={setRecommendation}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="pending">Решение не принято</SelectItem>
                <SelectItem value="advance">Перейти дальше</SelectItem>
                <SelectItem value="hold">Отложить</SelectItem>
                <SelectItem value="decline">Отказать</SelectItem>
              </SelectContent>
            </Select>
            <Button disabled={!reviewerName.trim()} onClick={() => void saveDebrief()}>{t('Save my debrief')}</Button>
            {debriefs.length > 0 && <p className="text-xs text-muted-foreground">{debriefs.length} {t('debriefs saved; individual opinions stay hidden during independent review')}</p>}
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-semibold">{t('Multi-stage interview process')}</h3>
            <div className="grid gap-2 sm:grid-cols-2"><Input value={trackCandidate} placeholder={t('Candidate name')} onChange={(e) => setTrackCandidate(e.target.value)} /><Input value={trackRole} placeholder={t('Role')} onChange={(e) => setTrackRole(e.target.value)} /></div>
            <Button variant="outline" onClick={() => void createTrack()}>{t('Create process')}</Button>
            <div className="grid gap-2 sm:grid-cols-3"><Input value={trackId} placeholder={t('Process ID')} onChange={(e) => setTrackId(e.target.value)} /><Input type="number" value={stageOrder} onChange={(e) => setStageOrder(e.target.value)} /><Input value={stageName} placeholder={t('Stage name')} onChange={(e) => setStageName(e.target.value)} /></div>
            <Button variant="outline" disabled={!trackId} onClick={() => void assignStage()}>{t('Link this stage')}</Button>
            {handoff && handoff.open_questions.length > 0 && <div className="rounded-lg bg-muted p-3"><p className="text-xs font-semibold">{t('Handoff from previous stages')}</p>{handoff.open_questions.map((item, index) => <p className="mt-1 text-xs" key={`${item.question}-${index}`}>• {item.question} · {item.source_meeting_title}</p>)}</div>}
          </div>

          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => void exportMemory('internal')}><Download size={15} />{t('Internal export')}</Button>
            <Button variant="outline" disabled={!privacy.candidateExportAllowed} onClick={() => void exportMemory('candidate')}><Download size={15} />{t('Candidate-safe export')}</Button>
          </div>
        </div>
      )}
    </section>
  );
}
