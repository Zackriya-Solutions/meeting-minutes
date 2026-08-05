"use client";

import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '@/components/ui/fluid-dialog';
import { Button } from '@/components/ui/button';
import { AlertCircle, Check, CheckCircle, Circle, Download, Loader2, RefreshCw } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import type {
  AnalyticsAnswer,
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

// Fallback Russian stage names (11 stages). Live
// labels from the progress events override these per stage — see the accumulation
// below. Kept in Russian to match the backend-provided stage labels regardless of
// UI language. Mirror of STAGE_META in src-tauri/src/report/pipeline.rs (same order
// and wording), so pending rows show the same name the stage will carry once live.
const FALLBACK_STAGE_LABELS = [
  'Анализ динамики разговора',
  'Классификация встречи',
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
  const { status, stageLabel, stageIndex, totalStages, error } = report;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="sm">
        {status === 'waiting_input' ? (
          <QuestionsView report={report} />
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

  // stage_index is 1-based (1…11), emitted at the start of each stage. Rows use a
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
                  <Check size={16} className="text-success" />
                ) : current ? (
                  <Loader2 size={16} className="animate-spin text-primary" />
                ) : (
                  <Circle size={12} className="text-muted-foreground" />
                )}
              </span>
              <span
                className={cn(
                  'truncate',
                  current ? 'font-semibold text-foreground' : done ? 'text-muted-foreground' : 'text-muted-foreground',
                )}
              >
                {label}
              </span>
            </li>
          );
        })}
      </ol>
      <DialogDescription className="text-xs text-muted-foreground">
        {t('Generation continues in background')}
      </DialogDescription>
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
              className="flex flex-col gap-2.5 rounded-[14px] border border-border bg-background p-3.5"
            >
              <p className="text-sm font-semibold text-foreground">{q.text}</p>

              {q.quote && (
                <blockquote className="border-l-2 border-border pl-3 text-[13px] italic text-muted-foreground">
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
                          ? 'border-primary/40 bg-primary text-primary-foreground'
                          : 'border-border bg-transparent text-foreground hover:bg-accent',
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
                  className="w-full rounded-[10px] border border-border bg-muted px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary/40"
                />
              )}

              {q.affects && (
                <p className="text-[11px] text-muted-foreground">
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
          className="border-primary/40 bg-primary text-primary-foreground hover:bg-primary/90"
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
  const { generate, downloadReport } = report;
  return (
    <>
      <DialogTitle>{t('Report ready')}</DialogTitle>
      <div className="flex flex-col items-center gap-3 py-2 text-center">
        <CheckCircle size={40} className="text-success" />
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
          className="border-primary/40 bg-primary text-primary-foreground hover:bg-primary/90"
          onClick={() => void downloadReport()}
        >
          <Download size={16} />
          {t('Download report')}
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
        <AlertCircle size={40} className="text-destructive" />
        {error && <DialogDescription className="break-words">{error}</DialogDescription>}
      </div>
      <div className="flex justify-end">
        <Button
          variant="outline"
          size="sm"
          className="border-primary/40 bg-primary text-primary-foreground hover:bg-primary/90"
          onClick={() => void report.generate()}
        >
          <RefreshCw size={16} />
          {t('Retry')}
        </Button>
      </div>
    </>
  );
}
