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

/** Which interactive pause the pipeline is parked on while `waiting_input`. */
export type AnalyticsWaitingKind = 'clarify';

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


export interface UseAnalyticsReportResult {
  /** The latest persisted report state has been restored for the current meeting. */
  hydrated: boolean;
  status: AnalyticsReportStatus;
  stageLabel: string;
  stageIndex: number;
  totalStages: number;
  htmlPath: string | null;
  error: string | null;
  /** Clarifying questions to answer while status is `waiting_input`. */
  questions: AnalyticsQuestion[];
  /** Which pause screen `waiting_input` refers to (null when not waiting). */
  waitingKind: AnalyticsWaitingKind | null;
  /**
   * Start (or regenerate) the report. Optimistically enters the running state.
   * `autoDownload` (default true) offers the finished HTML through the save dialog — the
   * "⋯ → Аналитический отчёт" flow wants that, a build started to fill the meeting's own
   * analytics tabs does not.
   */
  generate: (options?: { autoDownload?: boolean; automatic?: boolean }) => Promise<void>;
  /** Cancel the in-flight report (no-op if nothing is running). */
  cancel: () => Promise<void>;
  /** Submit answers to the clarifying questions (empty array = skip all). */
  submitAnswers: (answers: AnalyticsAnswer[]) => Promise<void>;
  /** Open the meeting folder in Finder/Explorer with the report file selected. */
  revealReport: () => Promise<void>;
  /**
   * Open the generated HTML report itself (browser / default handler). Resolves the file
   * from the latest COMPLETED run, so a later failed regeneration does not hide it.
   */
  openReport: () => Promise<void>;
  /** Save a copy of the completed HTML report through the system file dialog. */
  downloadReport: () => Promise<void>;
}

// The pipeline has 11 stages; used as the optimistic default before the first
// progress event (and as a fallback if the backend omits a stage total).
const DEFAULT_TOTAL_STAGES = 11;

// The DB row stores the machine stage id (English); progress events carry the
// Russian label. Mirror of STAGE_META in src-tauri/src/report/pipeline.rs so
// polled/restored state displays the same Russian names as live events.
const STAGE_LABELS_RU: Record<string, string> = {
  dynamics: 'Анализ динамики разговора',
  classify: 'Классификация встречи',
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
  const [hydratedMeetingId, setHydratedMeetingId] = useState<string | null>(null);
  const [status, setStatus] = useState<AnalyticsReportStatus>('idle');
  const [stageLabel, setStageLabel] = useState('');
  const [stageIndex, setStageIndex] = useState(0);
  const [totalStages, setTotalStages] = useState(DEFAULT_TOTAL_STAGES);
  const [htmlPath, setHtmlPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [questions, setQuestions] = useState<AnalyticsQuestion[]>([]);
  const [waitingKind, setWaitingKind] = useState<AnalyticsWaitingKind | null>(null);

  // Tracked for commands that need the report id (not the meeting id), e.g.
  // cancel / submit answers. Kept in a ref so those callbacks stay stable.
  const reportIdRef = useRef<string | null>(null);
  // Only reports explicitly started from this mounted meeting view should open
  // the save dialog on completion. Restoring an already-completed report must
  // never summon a native dialog unexpectedly.
  const autoDownloadRequestedRef = useRef(false);

  const reset = useCallback(() => {
    reportIdRef.current = null;
    autoDownloadRequestedRef.current = false;
    setStatus('idle');
    setStageLabel('');
    setStageIndex(0);
    setTotalStages(DEFAULT_TOTAL_STAGES);
    setHtmlPath(null);
    setError(null);
    setQuestions([]);
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
    // Restore the pause screen after a reload: clarify is the only interactive pause.
    const kind: AnalyticsWaitingKind | null = uiStatus === 'waiting_input' ? 'clarify' : null;
    setWaitingKind(kind);
    setQuestions(kind ? parseJsonArray<AnalyticsQuestion>(meta.questions, 'questions') : []);
  }, [reset]);

  // Restore persisted state whenever the meeting changes: reset first so a stale
  // report from the previous meeting never flashes, then hydrate from the backend.
  useEffect(() => {
    reset();
    if (!meetingId) return;
    let active = true;
    invoke<AnalyticsReportMeta | null>('get_analytics_report', { meetingId })
      .then((meta) => { if (active) applyMeta(meta); })
      .catch((e) => { console.error('Failed to restore analytics report:', e); })
      .finally(() => { if (active) setHydratedMeetingId(meetingId); });
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
        void Analytics.track('analytics_report_completed', { success: 'true' });
      });
      unlistenError = await listen<AnalyticsErrorEvent>('analytics-report-error', (event) => {
        const p = event.payload;
        if (p.meeting_id !== meetingId) return;
        reportIdRef.current = p.report_id;
        autoDownloadRequestedRef.current = false;
        setStatus('failed');
        setError(p.error ?? 'Unknown error');
        void Analytics.trackError('analytics_report_failed', p.error ?? 'Unknown error');
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
    };

    setup();

    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenError?.();
      unlistenQuestions?.();
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

  const generate = useCallback(async (options?: { autoDownload?: boolean; automatic?: boolean }) => {
    if (!meetingId) return;
    if (!options?.automatic) {
      Analytics.trackButtonClick('generate_analytics_report', 'meeting_details');
    }
    // Optimistic running state so the button reacts immediately; real stage labels
    // arrive via the progress events.
    reportIdRef.current = null;
    autoDownloadRequestedRef.current = options?.autoDownload ?? true;
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
      autoDownloadRequestedRef.current = false;
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

  const revealReport = useCallback(async () => {
    if (!htmlPath) return;
    Analytics.trackButtonClick('reveal_analytics_report', 'meeting_details');
    try {
      await invoke('reveal_report_in_folder', { path: htmlPath });
    } catch (e) {
      console.error('Failed to reveal analytics report:', e);
    }
  }, [htmlPath]);

  const openReport = useCallback(async () => {
    if (!meetingId) return;
    Analytics.trackButtonClick('open_analytics_report', 'meeting_details');
    try {
      await invoke('open_analytics_report', { meetingId });
    } catch (e) {
      console.error('Failed to open analytics report:', e);
      toast.error(`${t('Failed to open report')}: ${String(e)}`);
    }
  }, [meetingId, t]);

  const downloadReport = useCallback(async () => {
    if (!meetingId || status !== 'completed') return;
    Analytics.trackButtonClick('download_analytics_report', 'meeting_details');
    try {
      const savedPath = await invoke<string | null>('download_analytics_report', { meetingId });
      if (savedPath) toast.success(t('Report saved'));
    } catch (e) {
      console.error('Failed to save analytics report:', e);
      toast.error(`${t('Failed to save report')}: ${String(e)}`);
    }
  }, [meetingId, status, t]);

  // A report started by the user is a single action: generate, then immediately
  // offer to save the completed HTML. Clear the intent before invoking so a
  // cancelled dialog or a duplicate completion signal cannot open it twice.
  useEffect(() => {
    if (status !== 'completed' || !autoDownloadRequestedRef.current) return;
    autoDownloadRequestedRef.current = false;
    void downloadReport();
  }, [status, downloadReport]);

  return {
    hydrated: Boolean(meetingId) && hydratedMeetingId === meetingId,
    status,
    stageLabel,
    stageIndex,
    totalStages,
    htmlPath,
    error,
    questions,
    waitingKind,
    generate,
    cancel,
    submitAnswers,
    revealReport,
    openReport,
    downloadReport,
  };
}
