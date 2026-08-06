import React, { useEffect } from 'react';
import { Check, Loader2, Mic, RefreshCw, Sparkles, Users } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useFinishOnboarding } from '../useFinishOnboarding';
import { useOnboarding } from '@/contexts/OnboardingContext';
import {
  SUMMARY_MODEL_FLASH,
  SUMMARY_MODEL_PRO,
} from '@/lib/onboarding-summary-model';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';

/**
 * The only setup screen: one download and one choice.
 *
 * Transcription and speaker recognition used to be two opt-in cards next to a third for a
 * local summary model — three decisions before the first recording, each with its own button.
 * They are one package now, it starts on its own, and the only thing left to decide is which
 * DeepSeek tier writes the summaries.
 */
export function ModelSetupStep() {
  const t = useT();
  const {
    isMac,
    totalSteps,
    goNext,
    modelPack,
    transcriptionLabel,
    startModelPack,
    summaryModel,
    setSummaryModel,
  } = useOnboarding();
  const { finish, isFinishing } = useFinishOnboarding();

  // Auto-start: reaching this screen is the consent. Nothing here is optional — without the
  // models there is no transcript at all.
  useEffect(() => {
    if (modelPack.status === 'idle') {
      void startModelPack();
    }
  }, [modelPack.status, startModelPack]);

  const isDownloading = modelPack.status === 'downloading';
  const isReady = modelPack.status === 'ready';

  const rows = [
    {
      icon: <Mic className="h-4 w-4" />,
      title: t('Speech recognition'),
      detail: transcriptionLabel,
      done: modelPack.transcriptionReady,
    },
    {
      icon: <Users className="h-4 w-4" />,
      title: t('Speaker recognition'),
      detail: t('Labels who said what'),
      done: modelPack.speakersReady,
    },
  ];

  const summaryOptions = [
    {
      model: SUMMARY_MODEL_PRO,
      title: 'DeepSeek v4 Pro',
      detail: t('More precise with details and figures'),
      badge: t('by default'),
    },
    {
      model: SUMMARY_MODEL_FLASH,
      title: 'DeepSeek v4 Flash',
      detail: t('Faster and cheaper'),
      badge: null,
    },
  ];

  const handleContinue = () => {
    if (isMac) {
      goNext();
    } else {
      void finish();
    }
  };

  return (
    <OnboardingContainer
      title={t('Models and summaries')}
      description={t('Speech is recognised on this computer. Summaries are written by DeepSeek, so they need the internet.')}
      step={2}
      totalSteps={totalSteps}
    >
      <div className="flex flex-col items-center space-y-6">
        {/* The package */}
        <div className="w-full max-w-lg rounded-xl border border-border bg-background p-5">
          <div className="mb-4 flex items-start justify-between gap-3">
            <div>
              <h3 className="font-medium text-foreground">{t('Recognition models')}</h3>
              <p className="text-sm text-muted-foreground">
                {isReady
                  ? t('Downloaded — transcription works without the internet')
                  : t('Downloaded once, then transcription works without the internet')}
              </p>
            </div>
            {isReady ? (
              <div className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-full bg-success/10">
                <Check className="h-4 w-4 text-success" />
              </div>
            ) : isDownloading ? (
              <Loader2 className="h-5 w-5 flex-shrink-0 animate-spin text-muted-foreground" />
            ) : null}
          </div>

          {!isReady && (
            <div className="mb-4 space-y-2">
              <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-muted-foreground transition-all duration-300"
                  style={{ width: `${modelPack.percent}%` }}
                />
              </div>
              <div className="flex items-center justify-between text-sm text-muted-foreground">
                <span className="tabular-nums">
                  {Math.round(modelPack.downloadedMb)} {t('MB of')} {Math.round(modelPack.totalMb)}{' '}
                  {t('MB')}
                </span>
                <span className="font-semibold tabular-nums text-foreground">
                  {Math.round(modelPack.percent)}%
                </span>
              </div>
            </div>
          )}

          <div className="space-y-2">
            {rows.map((row) => (
              <div key={row.title} className="flex items-start gap-3 text-sm">
                <span
                  className={cn(
                    'flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full',
                    row.done ? 'bg-success/10 text-success' : 'bg-muted text-muted-foreground',
                  )}
                >
                  {row.done ? <Check className="h-4 w-4" /> : row.icon}
                </span>
                {/* The variant label carries its own qualifiers ("Neural Engine, macOS 14+"),
                    so it has to wrap — clipping it hides which model is actually installed. */}
                <span className="min-w-0 flex-1 leading-relaxed">
                  <span className="text-foreground">{row.title}</span>{' '}
                  <span className="text-muted-foreground">{row.detail}</span>
                </span>
              </div>
            ))}
          </div>

          {modelPack.status === 'error' && (
            <div className="mt-4 rounded-md border border-destructive/40 bg-destructive/10 p-3">
              <p className="text-sm font-medium text-destructive">{t('Download Error')}</p>
              <p className="mt-1 text-xs text-destructive">{modelPack.error}</p>
              <button
                onClick={() => void startModelPack()}
                className="mt-3 flex h-9 w-full items-center justify-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
              >
                <RefreshCw size={16} />
                {t('Try Again')}
              </button>
            </div>
          )}
        </div>

        {/* The choice */}
        <div className="w-full max-w-lg space-y-3">
          <div className="flex items-center gap-2">
            <Sparkles className="h-4 w-4 text-muted-foreground" />
            <h3 className="font-medium text-foreground">{t('Model for summaries')}</h3>
          </div>

          <div className="grid gap-2 sm:grid-cols-2">
            {summaryOptions.map((option) => {
              const selected = summaryModel === option.model;
              return (
                <button
                  key={option.model}
                  type="button"
                  onClick={() => setSummaryModel(option.model)}
                  aria-pressed={selected}
                  className={cn(
                    'rounded-xl border p-4 text-left transition-colors',
                    selected
                      ? 'border-primary bg-primary/5'
                      : 'border-border bg-background hover:bg-muted',
                  )}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-foreground">{option.title}</span>
                    {selected && <Check className="h-4 w-4 flex-shrink-0 text-primary" />}
                  </div>
                  <p className="mt-1 text-sm text-muted-foreground">{option.detail}</p>
                  {option.badge && (
                    <p className="mt-2 text-xs text-muted-foreground">{option.badge}</p>
                  )}
                </button>
              );
            })}
          </div>

          <p className="text-xs text-muted-foreground">
            {t('You can change the model later in Settings → Summaries')}
          </p>
        </div>

        <div className="w-full max-w-xs">
          <Button
            onClick={handleContinue}
            disabled={isFinishing}
            className="h-11 w-full rounded-full"
          >
            {isFinishing ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : isDownloading ? (
              t('Continue — download keeps going')
            ) : (
              t('Continue')
            )}
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
