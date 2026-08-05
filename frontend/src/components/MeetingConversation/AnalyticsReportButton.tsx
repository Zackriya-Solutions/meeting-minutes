"use client";

import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { AlertCircle, FileText, FolderOpen, Loader2, RefreshCw } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { useAnalyticsReport } from '@/hooks/meeting-details/useAnalyticsReport';
import { AnalyticsReportDialog } from './AnalyticsReportDialog';

/**
 * Deep Analytics report control for the meeting top bar. A single compact button
 * whose icon + label track the pipeline state; the full build experience (stage
 * checklist, clarify questions, completion) lives in <AnalyticsReportDialog>.
 *
 *   idle          → «Аналитический отчёт»      → generate() + open dialog
 *   running       → «Собираем отчёт · n/N»     → reopen dialog (runs in background)
 *   waiting_input → «Спикеры · подтвердить» / «Есть вопросы · ответить»
 *                                              → open dialog on the pause screen
 *   completed     → «Показать в Finder»        → reveal file in Finder; ⟳ regenerates
 *   failed        → «Не удалось собрать отчёт» → open dialog (error + retry)
 *
 * On mount the hook restores any existing/running report from the backend.
 */

interface AnalyticsReportButtonProps {
  meetingId: string;
  /** Disable the idle action (e.g. no transcript to analyse yet). */
  disabled?: boolean;
  /** Tooltip explaining why the button is disabled. */
  disabledTitle?: string;
}

const LABEL_VISIBILITY = 'hidden md:inline';

export function AnalyticsReportButton({ meetingId, disabled = false, disabledTitle }: AnalyticsReportButtonProps) {
  const t = useT();
  const report = useAnalyticsReport(meetingId);
  const { status, stageLabel, stageIndex, totalStages, error, generate, revealReport } = report;
  const [dialogOpen, setDialogOpen] = useState(false);

  const generateAndOpen = () => {
    void generate();
    setDialogOpen(true);
  };

  let control: React.ReactNode;

  if (status === 'completed') {
    control = (
      <div className="flex shrink-0 items-center gap-1">
        <Button variant="outline" size="sm" onClick={() => void revealReport()} title={t('Show in Finder')}>
          <FolderOpen size={16} />
          <span className={LABEL_VISIBILITY}>{t('Show in Finder')}</span>
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-9 w-9 rounded-full"
          onClick={generateAndOpen}
          title={t('Regenerate report')}
          aria-label={t('Regenerate report')}
        >
          <RefreshCw size={16} />
        </Button>
      </div>
    );
  } else if (status === 'waiting_input') {
    const waitingLabel = t('Questions — tap to answer');
    control = (
      <Button
        variant="outline"
        size="sm"
        className="shrink-0 animate-pulse border-primary/40 bg-primary/10 text-foreground hover:bg-primary"
        onClick={() => setDialogOpen(true)}
        title={waitingLabel}
      >
        <AlertCircle size={16} />
        <span className={LABEL_VISIBILITY}>{waitingLabel}</span>
      </Button>
    );
  } else if (status === 'running') {
    const progress = `${t('Building report')} · ${stageIndex}/${totalStages}`;
    control = (
      <Button
        variant="outline"
        size="sm"
        className="shrink-0"
        onClick={() => setDialogOpen(true)}
        title={stageLabel || progress}
      >
        <Loader2 size={16} className="animate-spin" />
        <span className={cn(LABEL_VISIBILITY, 'mm-numeric')}>{progress}</span>
      </Button>
    );
  } else if (status === 'failed') {
    control = (
      <Button
        variant="outline"
        size="sm"
        className="shrink-0 border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive"
        onClick={() => setDialogOpen(true)}
        title={error ? `${t('Report failed')}: ${error}` : t('Report failed')}
      >
        <AlertCircle size={16} />
        <span className={LABEL_VISIBILITY}>{t('Report failed')}</span>
      </Button>
    );
  } else {
    // idle
    control = (
      <Button
        variant="outline"
        size="sm"
        className="shrink-0"
        onClick={generateAndOpen}
        disabled={disabled}
        title={disabled ? (disabledTitle ?? t('Analytical report')) : t('Analytical report')}
      >
        <FileText size={16} />
        <span className={LABEL_VISIBILITY}>{t('Analytical report')}</span>
      </Button>
    );
  }

  return (
    <>
      {control}
      <AnalyticsReportDialog open={dialogOpen} onOpenChange={setDialogOpen} report={report} />
    </>
  );
}
