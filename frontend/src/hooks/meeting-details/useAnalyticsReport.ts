import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import Analytics from '@/lib/analytics';

/**
 * Drives the multi-stage "Аналитический отчёт" (Deep Analytics report) pipeline
 * for a single meeting. The Rust backend owns the pipeline; this hook mirrors its
 * state into React via:
 *   1. an initial `get_analytics_report` restore on `meetingId` change (a report
 *      may already exist or be running from a previous session), and
 *   2. `analytics-report-*` Tauri events (progress / complete / error), plus
 *   3. a 5s `get_analytics_report` poll while running as a fallback for missed events.
 *
 * Backend contract is frozen — see the command / event names below. Invoke args use
 * camelCase (Tauri v2 auto-converts to the snake_case Rust params), matching the
 * app's existing invoke style (e.g. `invoke('api_get_summary', { meetingId })`).
 */

type BackendStatus = 'queued' | 'running' | 'waiting_input' | 'completed' | 'failed' | 'cancelled';

/** UI-facing status collapsing the backend states the button cares about. */
export type AnalyticsReportStatus = 'idle' | 'running' | 'waiting_input' | 'completed' | 'failed';

/** A clarifying question the pipeline pauses on (status `waiting_input`). */
export interface AnalyticsQuestion {
  id: string;
  kind: 'ambiguity' | 'context';
  text: string;
  quote: string | null;
  options: string[];
  affects: string | null;
}

/** One answer submitted back to the pipeline (snake_case keys per the contract). */
export interface AnalyticsAnswer {
  question_id: string;
  answer: string | null;
}

/** One transcript line quoted inside the speaker-confirmation dialog. */
export interface AnalyticsSpeakerLine {
  seg: number;
  /** mm:ss offset from the recording start. */
  time: string;
  speaker_id: number | null;
  label: string;
  text: string;
  /** The line an excerpt was built around (e.g. the name evidence). */
  highlight: boolean;
}

/**
 * One speaker row of the speaker-confirmation pause (status `waiting_input`,
 * stage `speakers`): the meeting's speaker, the LLM's name/merge suggestion, and
 * the transcript excerpts needed to judge both.
 */
export interface AnalyticsSpeakerSuggestion {
  speaker_id: number;
  current_name: string;
  segment_count: number;
  is_confirmed: boolean;
  suggested_name: string | null;
  confidence: number;
  evidence: string | null;
  /** The LLM believes this speaker is the same person as this other speaker id. */
  merge_into: number | null;
  merge_reason: string | null;
  /** Share of total speech time, 0..1. */
  talk_share: number;
  /** mm:ss of this speaker's first line. */
  first_seen: string;
  /** Representative lines spread across the meeting (who this speaker is). */
  samples: AnalyticsSpeakerLine[];
  /** Dialogue around the line the name guess came from. */
  evidence_context: AnalyticsSpeakerLine[];
  /** Excerpt where this speaker and the proposed merge target both talk. */
  merge_context: AnalyticsSpeakerLine[];
}

/** One speaker decision submitted back (snake_case keys per the contract). */
export interface AnalyticsSpeakerDecision {
  speaker_id: number;
  /** Final display name; null = keep the current one. */
  display_name: string | null;
  /** Fold this speaker into another speaker id; null = stays separate. */
  merge_into: number | null;
}

/** Which interactive pause the pipeline is parked on while `waiting_input`. */
export type AnalyticsWaitingKind = 'speakers' | 'clarify';

/** Persisted report metadata returned by `get_analytics_report`. */
export interface AnalyticsReportMeta {
  id: string;
  meeting_id: string;
  status: BackendStatus;
  stage: string | null;
  stage_index: number;
  total_stages: number;
  html_path: string | null;
  error: string | null;
  created_at: string;
  completed_at: string | null;
  /** Raw JSON string of the questions array; present when status is `waiting_input`. */
  questions: string | null;
  /** Raw JSON string of the speaker suggestions array (speakers-stage pause). */
  speaker_suggestions: string | null;
}

interface AnalyticsProgressEvent {
  report_id: string;
  meeting_id: string;
  stage: string;
  stage_index: number;
  total_stages: number;
  label: string;
}

interface AnalyticsCompleteEvent {
  report_id: string;
  meeting_id: string;
  html_path: string;
}

interface AnalyticsErrorEvent {
  report_id: string;
  meeting_id: string;
  error: string;
}

interface AnalyticsQuestionsEvent {
  report_id: string;
  meeting_id: string;
  questions: AnalyticsQuestion[];
}

interface AnalyticsSpeakersEvent {
  report_id: string;
  meeting_id: string;
  speakers: AnalyticsSpeakerSuggestion[];
}

export interface UseAnalyticsReportResult {
  status: AnalyticsReportStatus;
  stageLabel: string;
  stageIndex: number;
  totalStages: number;
  htmlPath: string | null;
  error: string | null;
  /** Clarifying questions to answer while status is `waiting_input`. */
  questions: AnalyticsQuestion[];
  /** Speaker suggestions to confirm while status is `waiting_input` (speakers stage). */
  speakers: AnalyticsSpeakerSuggestion[];
  /** Which pause screen `waiting_input` refers to (null when not waiting). */
  waitingKind: AnalyticsWaitingKind | null;
  /** Start (or regenerate) the report. Optimistically enters the running state. */
  generate: () => Promise<void>;
  /** Cancel the in-flight report (no-op if nothing is running). */
  cancel: () => Promise<void>;
  /** Submit answers to the clarifying questions (empty array = skip all). */
  submitAnswers: (answers: AnalyticsAnswer[]) => Promise<void>;
  /** Submit speaker decisions (empty array = skip, change nothing). */
  submitSpeakers: (decisions: AnalyticsSpeakerDecision[]) => Promise<void>;
  /** Open the meeting folder in Finder/Explorer with the report file selected. */
  revealReport: () => Promise<void>;
}

// The pipeline has 13 stages; used as the optimistic default before the first
// progress event (and as a fallback if the backend omits a stage total).
const DEFAULT_TOTAL_STAGES = 13;

// The DB row stores the machine stage id (English); progress events carry the
// Russian label. Mirror of STAGE_META in src-tauri/src/report/pipeline.rs so
// polled/restored state displays the same Russian names as live events.
const STAGE_LABELS_RU: Record<string, string> = {
  speakers: 'Определение спикеров',
  dynamics: 'Анализ динамики разговора',
  classify: 'Классификация встречи',
  clarify: 'Уточняющие вопросы',
  topics: 'Темы и повестка',
  decisions: 'Решения',
  commitments: 'Обязательства',
  threads_risks: 'Незакрытое и риски',
  disagreements_concepts: 'Разногласия и концепции',
  numbers: 'Числа встречи',
  roles: 'Роли на встрече',
  insights: 'Главное — синтез',
  render: 'Сборка отчёта',
};

function toUiStatus(status: BackendStatus): AnalyticsReportStatus {
  switch (status) {
    case 'queued':
    case 'running':
      return 'running';
    case 'waiting_input':
      return 'waiting_input';
    case 'completed':
      return 'completed';
    case 'failed':
      return 'failed';
    case 'cancelled':
    default:
      return 'idle';
  }
}

/**
 * Backfill fields a suggestion persisted by an older build won't have. A report parked
 * in `waiting_input` across an app upgrade is restored from that older JSON, so the
 * excerpt arrays can be missing entirely.
 */
function normalizeSpeakers(list: AnalyticsSpeakerSuggestion[]): AnalyticsSpeakerSuggestion[] {
  return list.map((s) => ({
    ...s,
    talk_share: s.talk_share ?? 0,
    first_seen: s.first_seen ?? '',
    samples: s.samples ?? [],
    evidence_context: s.evidence_context ?? [],
    merge_context: s.merge_context ?? [],
  }));
}

/** Parse a raw JSON array string from meta; tolerant of null / malformed. */
function parseJsonArray<T>(raw: string | null | undefined, what: string): T[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as T[]) : [];
  } catch (e) {
    console.warn(`Failed to parse analytics report ${what}:`, e);
    return [];
  }
}

export function useAnalyticsReport(meetingId: string | null): UseAnalyticsReportResult {
  const t = useT();
  const [status, setStatus] = useState<AnalyticsReportStatus>('idle');
  const [stageLabel, setStageLabel] = useState('');
  const [stageIndex, setStageIndex] = useState(0);
  const [totalStages, setTotalStages] = useState(DEFAULT_TOTAL_STAGES);
  const [htmlPath, setHtmlPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [questions, setQuestions] = useState<AnalyticsQuestion[]>([]);
  const [speakers, setSpeakers] = useState<AnalyticsSpeakerSuggestion[]>([]);
  const [waitingKind, setWaitingKind] = useState<AnalyticsWaitingKind | null>(null);

  // Tracked for commands that need the report id (not the meeting id), e.g.
  // cancel / submit answers. Kept in a ref so those callbacks stay stable.
  const reportIdRef = useRef<string | null>(null);

  const reset = useCallback(() => {
    reportIdRef.current = null;
    setStatus('idle');
    setStageLabel('');
    setStageIndex(0);
    setTotalStages(DEFAULT_TOTAL_STAGES);
    setHtmlPath(null);
    setError(null);
    setQuestions([]);
    setSpeakers([]);
    setWaitingKind(null);
  }, []);

  const applyMeta = useCallback((meta: AnalyticsReportMeta | null) => {
    if (!meta) {
      reset();
      return;
    }
    const uiStatus = toUiStatus(meta.status);
    reportIdRef.current = meta.id;
    setStatus(uiStatus);
    setStageLabel(meta.stage ? STAGE_LABELS_RU[meta.stage] ?? meta.stage : '');
    setStageIndex(meta.stage_index ?? 0);
    setTotalStages(meta.total_stages || DEFAULT_TOTAL_STAGES);
    setHtmlPath(meta.html_path ?? null);
    setError(meta.error ?? null);
    // Restore the pause screen after a reload. The `stage` column disambiguates
    // which pause `waiting_input` refers to: the speakers confirmation or clarify.
    const kind: AnalyticsWaitingKind | null =
      uiStatus === 'waiting_input' ? (meta.stage === 'speakers' ? 'speakers' : 'clarify') : null;
    setWaitingKind(kind);
    setQuestions(kind === 'clarify' ? parseJsonArray<AnalyticsQuestion>(meta.questions, 'questions') : []);
    setSpeakers(
      kind === 'speakers'
        ? normalizeSpeakers(
            parseJsonArray<AnalyticsSpeakerSuggestion>(meta.speaker_suggestions, 'speaker suggestions'),
          )
        : [],
    );
  }, [reset]);

  // Restore persisted state whenever the meeting changes: reset first so a stale
  // report from the previous meeting never flashes, then hydrate from the backend.
  useEffect(() => {
    reset();
    if (!meetingId) return;
    let active = true;
    invoke<AnalyticsReportMeta | null>('get_analytics_report', { meetingId })
      .then((meta) => { if (active) applyMeta(meta); })
      .catch((e) => { console.error('Failed to restore analytics report:', e); });
    return () => { active = false; };
  }, [meetingId, reset, applyMeta]);

  // Live progress via Tauri events. Every payload is filtered by meeting_id so a
  // report running for another meeting can never leak into this view.
  useEffect(() => {
    if (!meetingId) return;
    let unlistenProgress: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unlistenQuestions: (() => void) | undefined;
    let unlistenSpeakers: (() => void) | undefined;

    const setup = async () => {
      unlistenProgress = await listen<AnalyticsProgressEvent>('analytics-report-progress', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        setStatus('running');
        setWaitingKind(null);
        setStageLabel(p.label ?? '');
        setStageIndex(p.stage_index ?? 0);
        setTotalStages(p.total_stages || DEFAULT_TOTAL_STAGES);
        setError(null);
      });
      unlistenComplete = await listen<AnalyticsCompleteEvent>('analytics-report-complete', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        setStatus('completed');
        setHtmlPath(p.html_path ?? null);
        setError(null);
      });
      unlistenError = await listen<AnalyticsErrorEvent>('analytics-report-error', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        setStatus('failed');
        setError(p.error ?? 'Unknown error');
      });
      unlistenQuestions = await listen<AnalyticsQuestionsEvent>('analytics-report-questions', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        setStatus('waiting_input');
        setWaitingKind('clarify');
        setQuestions(Array.isArray(p.questions) ? p.questions : []);
        setError(null);
      });
      unlistenSpeakers = await listen<AnalyticsSpeakersEvent>('analytics-report-speakers', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        setStatus('waiting_input');
        setWaitingKind('speakers');
        setSpeakers(normalizeSpeakers(Array.isArray(p.speakers) ? p.speakers : []));
        setError(null);
      });
    };

    setup();

    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenError?.();
      unlistenQuestions?.();
      unlistenSpeakers?.();
    };
  }, [meetingId]);

  // Fallback poll while the pipeline is active, in case an event is missed. Covers
  // both 'running' and 'waiting_input' so a missed questions event (or a backend
  // auto-resume after the input timeout) is still picked up. Stops on any terminal
  // state (the effect re-runs when `status` changes) and on unmount.
  useEffect(() => {
    if (!meetingId || (status !== 'running' && status !== 'waiting_input')) return;
    let active = true;
    const intervalId = setInterval(async () => {
      try {
        const meta = await invoke<AnalyticsReportMeta | null>('get_analytics_report', { meetingId });
        if (active && meta) applyMeta(meta);
      } catch (e) {
        console.warn('Analytics report poll failed:', e);
      }
    }, 5000);
    return () => { active = false; clearInterval(intervalId); };
  }, [meetingId, status, applyMeta]);

  const generate = useCallback(async () => {
    if (!meetingId) return;
    Analytics.trackButtonClick('generate_analytics_report', 'meeting_details');
    // Optimistic running state so the button reacts immediately; real stage labels
    // arrive via the progress events.
    reportIdRef.current = null;
    setStatus('running');
    setStageLabel('Подготовка');
    setStageIndex(0);
    setTotalStages(DEFAULT_TOTAL_STAGES);
    setHtmlPath(null);
    setError(null);
    try {
      const res = await invoke<{ report_id: string }>('generate_analytics_report', { meetingId });
      reportIdRef.current = res?.report_id ?? null;
    } catch (e) {
      console.error('Failed to start analytics report:', e);
      setStatus('failed');
      setError(e instanceof Error ? e.message : String(e));
      toast.error(t('Report failed'));
    }
  }, [meetingId, t]);

  const cancel = useCallback(async () => {
    const reportId = reportIdRef.current;
    if (!reportId) return;
    Analytics.trackButtonClick('cancel_analytics_report', 'meeting_details');
    try {
      await invoke('cancel_analytics_report', { reportId });
    } catch (e) {
      console.error('Failed to cancel analytics report:', e);
    }
    reset();
  }, [reset]);

  const submitAnswers = useCallback(async (answers: AnalyticsAnswer[]) => {
    const reportId = reportIdRef.current;
    if (!reportId) return;
    Analytics.trackButtonClick(
      answers.length === 0 ? 'skip_analytics_questions' : 'submit_analytics_answers',
      'meeting_details',
    );
    // Optimistically return to the running checklist; the pipeline resumes and the
    // next progress events (or the poll) drive the stages forward.
    setStatus('running');
    setQuestions([]);
    setWaitingKind(null);
    try {
      await invoke('submit_analytics_answers', { reportId, answers });
    } catch (e) {
      console.error('Failed to submit analytics answers:', e);
      setStatus('failed');
      setError(e instanceof Error ? e.message : String(e));
      toast.error(t('Report failed'));
    }
  }, [t]);

  const submitSpeakers = useCallback(async (decisions: AnalyticsSpeakerDecision[]) => {
    const reportId = reportIdRef.current;
    if (!reportId) return;
    Analytics.trackButtonClick(
      decisions.length === 0 ? 'skip_analytics_speakers' : 'submit_analytics_speakers',
      'meeting_details',
    );
    setStatus('running');
    setSpeakers([]);
    setWaitingKind(null);
    try {
      await invoke('submit_analytics_speakers', { reportId, decisions });
    } catch (e) {
      console.error('Failed to submit analytics speaker decisions:', e);
      setStatus('failed');
      setError(e instanceof Error ? e.message : String(e));
      toast.error(t('Report failed'));
    }
  }, [t]);

  const revealReport = useCallback(async () => {
    if (!htmlPath) return;
    Analytics.trackButtonClick('reveal_analytics_report', 'meeting_details');
    try {
      await invoke('reveal_report_in_folder', { path: htmlPath });
    } catch (e) {
      console.error('Failed to reveal analytics report:', e);
    }
  }, [htmlPath]);

  return {
    status,
    stageLabel,
    stageIndex,
    totalStages,
    htmlPath,
    error,
    questions,
    speakers,
    waitingKind,
    generate,
    cancel,
    submitAnswers,
    submitSpeakers,
    revealReport,
  };
}
