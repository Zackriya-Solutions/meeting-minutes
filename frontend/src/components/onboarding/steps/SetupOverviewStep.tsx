import React, { useEffect, useState } from 'react';
import { Info } from '@/components/memento/LucideCompat';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export function SetupOverviewStep() {
  const { goNext } = useOnboarding();
  const t = useT();
  const [isMac, setIsMac] = useState(false);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  const steps = [
    {
      number: 1,
      type: 'transcription',
      title: 'Download Transcription Engine',
    },
    {
      number: 2,
      type: 'summarization',
      title: 'Download Summarization Engine',
    },
  ];

  const handleContinue = () => {
    goNext();
  };

  return (
    <OnboardingContainer
      title={t('Setup Overview')}
      description={t('Memento requires transcription and summarization services to process meetings.')}
      step={2}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Steps Card */}
        <div className="w-full max-w-md bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-4">
          <div className="space-y-4">
            {steps.map((step, idx) => {
              return (
                <div
                  key={step.number}
                  className={`flex items-start gap-4 p-1`}
                >
                  <div className="flex-1 ml-1">
                    <h3 className="font-medium text-[var(--fg1)] flex items-center gap-2">
                        {t('Step')} {step.number} · {t(step.title)}

                        {step.type === "summarization" && (
                            <TooltipProvider>
                            <Tooltip>
                                <TooltipTrigger asChild>
                                <button className="text-[var(--fg3)] hover:text-[var(--fg2)]">
                                    <Info className="w-4 h-4" />
                                </button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs text-sm">
                                {t('You can also select external AI providers like OpenAI, Claude, or Ollama for summary generation in settings.')}
                                </TooltipContent>
                            </Tooltip>
                            </TooltipProvider>
                        )}
                        </h3>
                  </div>
                </div>
              );
            })}
          </div>
        </div>


        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-4">
          <Button
            onClick={handleContinue}
            className="h-11 w-full rounded-full bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]"
          >
            {t("Let's Go")}
          </Button>
          <div className="text-center">
            <a
              href="https://github.com/andyzt/meet_at_giga/issues"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-[var(--fg2)] hover:underline"
            >
              {t('Report issues on GitHub')}
            </a>
          </div>
        </div>
      </div>
    </OnboardingContainer>
  );
}
