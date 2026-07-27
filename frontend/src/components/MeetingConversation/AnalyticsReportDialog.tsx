"use client";

import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { AlertCircle, Check, CheckCircle, Circle, FolderOpen, Loader2, RefreshCw } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { localizeSpeakerLabel } from '@/types';
import type {
  AnalyticsAnswer,
  AnalyticsSpeakerDecision,
  AnalyticsSpeakerLine,
  UseAnalyticsReportResult,
} from '@/hooks/meeting-details/useAnalyticsReport';

/**
 * Hosts the whole "Аналитический отчёт" build experience in a modal. The report
 * hook is owned by the parent (AnalyticsReportButton) and passed in, so this stays
 * presentational and there is a single source of pipeline state.
 *
 * Closing the dialog never cancels and never skips — the pipeline keeps
 * running/waiting in the background, and reopening restores the current view
 * (running checklist / clarify questions / completed) from the shared hook state.
 */

// Fallback Russian stage names (13 stages; speakers runs 1st, clarify 4th). Live
// labels from the progress events override these per stage — see the accumulation
// below. Kept in Russian to match the backend-provided stage labels regardless of
// UI language. Mirror of STAGE_META in src-tauri/src/report/pipeline.rs (same order
// and wording), so pending rows show the same name the stage will carry once live.
const FALLBACK_STAGE_LABELS = [
  'Определение спикеров',
  'Анализ динамики разговора',
  'Классификация встречи',
  'Уточняющие вопросы',
  'Темы и повестка',
  'Решения',
  'Обязательства',
  'Незакрытое и риски',
  'Разногласия и концепции',
  'Числа встречи',
  'Роли на встрече',
  'Главное — синтез',
  'Сборка отчёта',
];

interface AnalyticsReportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  report: UseAnalyticsReportResult;
}

export function AnalyticsReportDialog({ open, onOpenChange, report }: AnalyticsReportDialogProps) {
  const { status, stageLabel, stageIndex, totalStages, error, waitingKind } = report;
  // The speaker step shows transcript excerpts side by side and needs the room.
  const isSpeakers = status === 'waiting_input' && waitingKind === 'speakers';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={cn('max-w-md', isSpeakers && 'max-w-2xl')}>
        {status === 'waiting_input' ? (
          waitingKind === 'speakers' ? <SpeakersView report={report} /> : <QuestionsView report={report} />
        ) : status === 'completed' ? (
          <CompletedView report={report} />
        ) : status === 'failed' ? (
          <FailedView report={report} error={error} />
        ) : (
          <RunningView status={status} stageLabel={stageLabel} stageIndex={stageIndex} totalStages={totalStages} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function RunningView({
  status,
  stageLabel,
  stageIndex,
  totalStages,
}: {
  status: string;
  stageLabel: string;
  stageIndex: number;
  totalStages: number;
}) {
  const t = useT();
  const count = totalStages || FALLBACK_STAGE_LABELS.length;

  // Accumulate the live label for each stage as its progress event arrives, so
  // already-passed stages keep their real names. Falls back to the hardcoded list
  // for stages not yet seen this session (e.g. after reopening mid-run).
  const [seenLabels, setSeenLabels] = useState<Record<number, string>>({});
  useEffect(() => {
    if (status !== 'running' || !stageLabel) return;
    setSeenLabels((prev) => (prev[stageIndex] === stageLabel ? prev : { ...prev, [stageIndex]: stageLabel }));
  }, [status, stageLabel, stageIndex]);

  // stage_index is 1-based (1…12), emitted at the start of each stage. Rows use a
  // 1-based position (i + 1): positions < activeIndex are done, activeIndex shows the
  // spinner, later positions are pending. The optimistic pre-event state (stageIndex 0,
  // «Подготовка») renders the spinner on row 1.
  const activeIndex = stageIndex > 0 ? stageIndex : 1;

  return (
    <>
      <DialogTitle>{t('Building report')}</DialogTitle>
      <ol className="flex flex-col gap-2">
        {Array.from({ length: count }).map((_, i) => {
          const position = i + 1;
          const done = position < activeIndex;
          const current = position === activeIndex;
          const label = seenLabels[position] ?? FALLBACK_STAGE_LABELS[i] ?? `${position}`;
          return (
            <li key={i} className="flex items-center gap-2.5 text-sm">
              <span className="flex h-5 w-5 shrink-0 items-center justify-center">
                {done ? (
                  <Check size={16} className="text-[var(--success)]" />
                ) : current ? (
                  <Loader2 size={16} className="animate-spin text-[var(--gold)]" />
                ) : (
                  <Circle size={12} className="text-[var(--fg3)]" />
                )}
              </span>
              <span
                className={cn(
                  'truncate',
                  current ? 'font-semibold text-[var(--fg1)]' : done ? 'text-[var(--fg2)]' : 'text-[var(--fg3)]',
                )}
              >
                {label}
              </span>
            </li>
          );
        })}
      </ol>
      <DialogDescription className="text-xs text-[var(--fg3)]">
        {t('Generation continues in background')}
      </DialogDescription>
    </>
  );
}

/** Sample lines shown per speaker before "show all". */
const SAMPLES_COLLAPSED = 2;

/**
 * A quoted transcript excerpt. `showLabels` prefixes each line with its speaker (for
 * multi-speaker excerpts); `focusId` marks the card's own speaker so the eye can
 * separate the two people being compared.
 */
function TranscriptLines({
  lines,
  localize,
  showLabels = false,
  focusId,
}: {
  lines: AnalyticsSpeakerLine[];
  localize: (name: string) => string;
  showLabels?: boolean;
  focusId?: number | null;
}) {
  return (
    <ul className="flex flex-col gap-1">
      {lines.map((line) => (
        <li
          key={line.seg}
          className={cn(
            'flex gap-2 text-[13px] leading-snug',
            line.highlight && '-mx-1 rounded-md bg-[var(--gold-soft)] px-1 py-0.5',
          )}
        >
          <span className="mm-numeric shrink-0 pt-px text-[11px] text-[var(--fg3)]">{line.time}</span>
          <span className="min-w-0 text-[var(--fg2)]">
            {showLabels && line.label && (
              <span
                className={cn(
                  'mr-1 font-semibold',
                  line.speaker_id === focusId ? 'text-[var(--fg1)]' : 'text-[var(--fg2)]',
                )}
              >
                {localize(line.label)}:
              </span>
            )}
            {line.text}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Small inline toggle used to expand excerpts inside a speaker card. */
function MoreToggle({ open, onClick, moreLabel, lessLabel }: {
  open: boolean;
  onClick: () => void;
  moreLabel: string;
  lessLabel: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="self-start text-[11px] text-[var(--fg3)] underline-offset-2 hover:text-[var(--fg2)] hover:underline"
    >
      {open ? lessLabel : moreLabel}
    </button>
  );
}

function SpeakersView({ report }: { report: UseAnalyticsReportResult }) {
  const t = useT();
  const { speakers, submitSpeakers } = report;

  // Draft name + merge target per speaker id, seeded from the LLM suggestions.
  const [names, setNames] = useState<Record<number, string>>({});
  const [merges, setMerges] = useState<Record<number, number | null>>({});
  // Per-card excerpt expansion.
  const [openSamples, setOpenSamples] = useState<Record<number, boolean>>({});
  const [openContext, setOpenContext] = useState<Record<number, boolean>>({});

  const speakersKey = speakers.map((s) => s.speaker_id).join('|');
  useEffect(() => {
    const seededNames: Record<number, string> = {};
    const seededMerges: Record<number, number | null> = {};
    for (const s of speakers) {
      seededNames[s.speaker_id] = s.suggested_name ?? s.current_name;
      seededMerges[s.speaker_id] = s.merge_into;
    }
    setNames(seededNames);
    setMerges(seededMerges);
    setOpenSamples({});
    setOpenContext({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [speakersKey]);

  const localized = (name: string) => localizeSpeakerLabel(name, t) ?? name;
  const draftLabel = (id: number) => {
    const s = speakers.find((x) => x.speaker_id === id);
    const draft = (names[id] ?? '').trim();
    return draft || (s ? localized(s.current_name) : String(id));
  };

  // Changing a merge keeps groups flat: anyone previously pointing at `id` follows
  // it to the new target (the backend drops chained merges).
  const setMerge = (id: number, target: number | null) => {
    setMerges((prev) => {
      const next = { ...prev, [id]: target };
      for (const s of speakers) {
        if (s.speaker_id !== id && next[s.speaker_id] === id) {
          next[s.speaker_id] = target;
        }
      }
      return next;
    });
  };

  const buildDecisions = (): AnalyticsSpeakerDecision[] =>
    speakers.map((s) => {
      const target = merges[s.speaker_id] ?? null;
      if (target != null) {
        return { speaker_id: s.speaker_id, display_name: null, merge_into: target };
      }
      const name = (names[s.speaker_id] ?? '').trim();
      return {
        speaker_id: s.speaker_id,
        display_name: name && name !== s.current_name ? name : null,
        merge_into: null,
      };
    });

  return (
    <>
      <DialogTitle>{t('Meeting speakers')}</DialogTitle>
      <DialogDescription>{t('Names and merges apply to the meeting and the report')}</DialogDescription>

      <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto pr-1">
        {speakers.map((s) => {
          const target = merges[s.speaker_id] ?? null;
          const merged = target != null;
          const targets = speakers.filter(
            (o) => o.speaker_id !== s.speaker_id && (merges[o.speaker_id] ?? null) == null,
          );
          // The backend excerpt is built for the pair the model proposed; if the user
          // picks a different target, compare that speaker's own samples instead.
          const modelPair = merged && target === s.merge_into && s.merge_context.length > 0;
          const targetInfo = merged ? speakers.find((o) => o.speaker_id === target) : undefined;
          const samplesOpen = openSamples[s.speaker_id] ?? false;
          const shownSamples = samplesOpen ? s.samples : s.samples.slice(0, SAMPLES_COLLAPSED);
          const contextOpen = openContext[s.speaker_id] ?? false;

          return (
            <div
              key={s.speaker_id}
              className={cn(
                'flex flex-col gap-2.5 rounded-[14px] border bg-[var(--bg-canvas)] p-3.5',
                merged ? 'border-[var(--gold-border)]' : 'border-[var(--border-subtle)]',
              )}
            >
              <div className="flex items-baseline justify-between gap-2">
                <p className="text-sm font-semibold text-[var(--fg1)]">{localized(s.current_name)}</p>
                {s.suggested_name && (
                  <span className="shrink-0 rounded-full bg-[var(--gold-soft)] px-2 py-0.5 text-[11px] text-[var(--fg2)]">
                    {t('confidence')} {Math.round((s.confidence ?? 0) * 100)}%
                  </span>
                )}
              </div>

              <p className="mm-numeric text-[11px] text-[var(--fg3)]">
                {s.segment_count} {t('replies')}
                {s.talk_share > 0 && ` · ${Math.round(s.talk_share * 100)}% ${t('of talk time')}`}
                {s.first_seen && ` · ${t('start')} ${s.first_seen}`}
              </p>

              {shownSamples.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  <TranscriptLines lines={shownSamples} localize={localized} />
                  {s.samples.length > SAMPLES_COLLAPSED && (
                    <MoreToggle
                      open={samplesOpen}
                      onClick={() =>
                        setOpenSamples((prev) => ({ ...prev, [s.speaker_id]: !samplesOpen }))
                      }
                      moreLabel={t('More replies')}
                      lessLabel={t('Collapse')}
                    />
                  )}
                </div>
              )}

              {!merged && (
                <input
                  type="text"
                  value={names[s.speaker_id] ?? ''}
                  onChange={(e) =>
                    setNames((prev) => ({ ...prev, [s.speaker_id]: e.target.value }))
                  }
                  placeholder={t('Name')}
                  className="w-full rounded-[10px] border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-3 py-1.5 text-sm text-[var(--fg1)] outline-none focus:border-[var(--gold-border)]"
                />
              )}

              {s.suggested_name && (s.evidence || s.evidence_context.length > 0) && (
                <div className="flex flex-col gap-1.5 rounded-[10px] bg-[var(--bg-elevated)] p-2.5">
                  <p className="text-[11px] text-[var(--fg3)]">{t('Where the name comes from')}</p>
                  {contextOpen && s.evidence_context.length > 0 ? (
                    <TranscriptLines
                      lines={s.evidence_context}
                      localize={localized}
                      showLabels
                      focusId={s.speaker_id}
                    />
                  ) : (
                    s.evidence && (
                      <blockquote className="border-l-2 border-[var(--border-strong)] pl-3 text-[13px] italic text-[var(--fg2)]">
                        {s.evidence}
                      </blockquote>
                    )
                  )}
                  {s.evidence_context.length > 0 && (
                    <MoreToggle
                      open={contextOpen}
                      onClick={() =>
                        setOpenContext((prev) => ({ ...prev, [s.speaker_id]: !contextOpen }))
                      }
                      moreLabel={t('Show in context')}
                      lessLabel={t('Collapse')}
                    />
                  )}
                </div>
              )}

              {speakers.length > 1 && (
                <div className="flex items-center gap-2">
                  <label className="shrink-0 text-[11px] text-[var(--fg3)]">{t('Merge with')}</label>
                  <select
                    value={merges[s.speaker_id] ?? ''}
                    onChange={(e) =>
                      setMerge(s.speaker_id, e.target.value === '' ? null : Number(e.target.value))
                    }
                    className="w-full rounded-[10px] border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-2 py-1 text-[13px] text-[var(--fg1)] outline-none focus:border-[var(--gold-border)]"
                  >
                    <option value="">{t('Keep separate')}</option>
                    {targets.map((o) => (
                      <option key={o.speaker_id} value={o.speaker_id}>
                        {draftLabel(o.speaker_id)}
                      </option>
                    ))}
                  </select>
                </div>
              )}

              {merged && s.merge_reason && target === s.merge_into && (
                <p className="text-[11px] text-[var(--fg3)]">{s.merge_reason}</p>
              )}

              {merged && (modelPair || (targetInfo?.samples.length ?? 0) > 0) && (
                <div className="flex flex-col gap-1.5 rounded-[10px] border border-dashed border-[var(--border-strong)] p-2.5">
                  <p className="text-[11px] text-[var(--fg3)]">{t('Compare the replies')}</p>
                  {modelPair ? (
                    <TranscriptLines
                      lines={s.merge_context}
                      localize={localized}
                      showLabels
                      focusId={s.speaker_id}
                    />
                  ) : (
                    targetInfo && (
                      <>
                        <p className="text-[11px] font-semibold text-[var(--fg2)]">
                          {draftLabel(targetInfo.speaker_id)}
                        </p>
                        <TranscriptLines
                          lines={targetInfo.samples.slice(0, SAMPLES_COLLAPSED)}
                          localize={localized}
                        />
                      </>
                    )
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button variant="ghost" size="sm" onClick={() => void submitSpeakers([])}>
          {t('Skip')}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="border-[var(--gold-border)] bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]"
          onClick={() => void submitSpeakers(buildDecisions())}
        >
          {t('Confirm and continue')}
        </Button>
      </div>
    </>
  );
}

const OTHER_OPTION_VALUES = new Set(['другое', 'другой', 'other']);

function isOtherOption(option: string, otherLabel: string): boolean {
  const normalized = option.trim().toLowerCase();
  return OTHER_OPTION_VALUES.has(normalized) || normalized === otherLabel.trim().toLowerCase();
}

function QuestionsView({ report }: { report: UseAnalyticsReportResult }) {
  const t = useT();
  const { questions, submitAnswers } = report;
  const otherLabel = t('Other');

  // Selected option per question id, plus free text when the "other" option is picked.
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [otherText, setOtherText] = useState<Record<string, string>>({});

  // Reset local selections whenever a new set of questions arrives.
  const questionKey = questions.map((q) => q.id).join('|');
  useEffect(() => {
    setSelected({});
    setOtherText({});
  }, [questionKey]);

  const buildAnswers = (): AnalyticsAnswer[] =>
    questions.map((q) => {
      const choice = selected[q.id];
      if (choice == null) return { question_id: q.id, answer: null };
      if (isOtherOption(choice, otherLabel)) {
        const free = (otherText[q.id] ?? '').trim();
        return { question_id: q.id, answer: free.length > 0 ? free : null };
      }
      return { question_id: q.id, answer: choice };
    });

  return (
    <>
      <DialogTitle>{t('Clarifying questions')}</DialogTitle>
      <DialogDescription>{t('Answers refine the report — all optional')}</DialogDescription>

      <div className="flex max-h-[52vh] flex-col gap-4 overflow-y-auto pr-1">
        {questions.map((q) => {
          const choice = selected[q.id];
          const otherPicked = choice != null && isOtherOption(choice, otherLabel);
          return (
            <div
              key={q.id}
              className="flex flex-col gap-2.5 rounded-[14px] border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-3.5"
            >
              <p className="text-sm font-semibold text-[var(--fg1)]">{q.text}</p>

              {q.quote && (
                <blockquote className="border-l-2 border-[var(--border-strong)] pl-3 text-[13px] italic text-[var(--fg2)]">
                  {q.quote}
                </blockquote>
              )}

              <div className="flex flex-wrap gap-1.5">
                {q.options.map((option) => {
                  const active = choice === option;
                  return (
                    <button
                      key={option}
                      type="button"
                      onClick={() => setSelected((prev) => ({ ...prev, [q.id]: active ? '' : option }))}
                      className={cn(
                        'rounded-full border px-3 py-1 text-[13px] transition-colors',
                        active
                          ? 'border-[var(--gold-border)] bg-[var(--gold)] text-[var(--fg-inverse)]'
                          : 'border-[var(--border-strong)] bg-transparent text-[var(--fg1)] hover:bg-[var(--state-hover-bg)]',
                      )}
                    >
                      {option}
                    </button>
                  );
                })}
              </div>

              {otherPicked && (
                <input
                  type="text"
                  value={otherText[q.id] ?? ''}
                  onChange={(e) => setOtherText((prev) => ({ ...prev, [q.id]: e.target.value }))}
                  placeholder={otherLabel}
                  className="w-full rounded-[10px] border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-3 py-1.5 text-sm text-[var(--fg1)] outline-none focus:border-[var(--gold-border)]"
                />
              )}

              {q.affects && (
                <p className="text-[11px] text-[var(--fg3)]">
                  {t('Affects')}: {q.affects}
                </p>
              )}
            </div>
          );
        })}
      </div>

      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button variant="ghost" size="sm" onClick={() => void submitAnswers([])}>
          {t('Skip questions')}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="border-[var(--gold-border)] bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]"
          onClick={() => void submitAnswers(buildAnswers())}
        >
          {t('Answer and continue')}
        </Button>
      </div>
    </>
  );
}

function CompletedView({ report }: { report: UseAnalyticsReportResult }) {
  const t = useT();
  const { generate, revealReport } = report;
  return (
    <>
      <DialogTitle>{t('Report ready')}</DialogTitle>
      <div className="flex flex-col items-center gap-3 py-2 text-center">
        <CheckCircle size={40} className="text-[var(--success)]" />
        <DialogDescription>{t('Report ready')}</DialogDescription>
      </div>
      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button variant="ghost" size="sm" onClick={() => void generate()}>
          <RefreshCw size={16} />
          {t('Regenerate')}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="border-[var(--gold-border)] bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]"
          onClick={() => void revealReport()}
        >
          <FolderOpen size={16} />
          {t('Show in Finder')}
        </Button>
      </div>
    </>
  );
}

function FailedView({ report, error }: { report: UseAnalyticsReportResult; error: string | null }) {
  const t = useT();
  return (
    <>
      <DialogTitle>{t('Report failed')}</DialogTitle>
      <div className="flex flex-col items-center gap-3 py-2 text-center">
        <AlertCircle size={40} className="text-[var(--danger)]" />
        {error && <DialogDescription className="break-words">{error}</DialogDescription>}
      </div>
      <div className="flex justify-end">
        <Button
          variant="outline"
          size="sm"
          className="border-[var(--gold-border)] bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]"
          onClick={() => void report.generate()}
        >
          <RefreshCw size={16} />
          {t('Retry')}
        </Button>
      </div>
    </>
  );
}
