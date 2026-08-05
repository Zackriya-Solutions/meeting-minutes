"use client";

import { BrainCircuit, RefreshCw } from '@/components/deslop-icons';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { FluidSpinner } from '@/components/ui/fluid-spinner';
import { useT } from '@/lib/i18n';
import type { UseAnalyticsReportResult } from '@/hooks/meeting-details/useAnalyticsReport';

/**
 * What the «Числа встречи» and «Динамика встречи» tabs show before a meeting has an
 * analytical report: what these tabs will contain and one button that starts the build.
 * The same pipeline backs the "⋯ → Аналитический отчёт" action, so a run started there
 * fills these tabs too — and progress shows up here either way.
 */

// FluidButton's icon slot takes numeric-only props, so the icons are adapted here (same
// wrapper pattern the summary message uses for its action buttons).
type ActionIconProps = { size?: number; strokeWidth?: number; className?: string };
const BuildIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <BrainCircuit size={size} strokeWidth={strokeWidth} className={className} />
);
const RetryIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <RefreshCw size={size} strokeWidth={strokeWidth} className={className} />
);

export function AnalyticsBuildPrompt({
  report,
  hasTranscript,
  loading = false,
}: {
  report: UseAnalyticsReportResult;
  hasTranscript: boolean;
  loading?: boolean;
}) {
  const t = useT();

  if (loading) {
    return (
      <Frame>
        <FluidSpinner className="h-4 w-4 text-primary" />
      </Frame>
    );
  }

  if (report.status === 'running' || report.status === 'waiting_input') {
    // Same "4/11" progress shorthand the "⋯" menu shows next to the report item.
    const stage = [
      report.stageLabel,
      report.stageIndex > 0 ? `${report.stageIndex}/${report.totalStages}` : null,
    ].filter(Boolean).join(' · ');
    return (
      <Frame>
        <div className="flex items-center gap-2 text-[length:var(--ui-body-font-size)] text-foreground">
          <FluidSpinner className="h-4 w-4 text-primary" />
          {t('Building report')}
        </div>
        {stage && <p className="text-xs text-[var(--primary-40)]">{stage}</p>}
        <p className="text-xs text-[var(--primary-40)]">{t('Generation continues in background')}</p>
      </Frame>
    );
  }

  const failed = report.status === 'failed';

  return (
    <Frame>
      <div className="flex flex-col gap-1.5">
        <p className="text-[length:var(--ui-body-font-size)] font-medium leading-[21px] text-foreground">
          {t('Meeting analysis is not built yet')}
        </p>
        <p className="text-[length:var(--ui-body-font-size)] leading-[21px] text-[var(--primary-60)]">
          {t('Score, figures and dynamics come from the analytical report.')}
        </p>
        {failed && report.error && (
          <p className="text-xs text-destructive">{report.error}</p>
        )}
        {!hasTranscript && (
          <p className="text-xs text-[var(--primary-40)]">{t('A transcript is needed first.')}</p>
        )}
      </div>
      <FluidButton
        type="button"
        variant="secondary"
        size="sm"
        leadingIcon={failed ? RetryIcon : BuildIcon}
        disabled={!hasTranscript}
        className="self-start text-[var(--primary-60)]"
        onClick={() => void report.generate({ autoDownload: false })}
      >
        {failed ? t('Retry') : t('Build analysis')}
      </FluidButton>
    </Frame>
  );
}

function Frame({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-4 rounded-[14px] bg-[var(--primary-5)] px-4 py-4">
      {children}
    </div>
  );
}
