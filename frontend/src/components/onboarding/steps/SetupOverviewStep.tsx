import React, { useEffect, useState } from 'react';
import { Info } from '@/components/memento/LucideCompat';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export function SetupOverviewStep() {
  const { goNext } = useOnboarding();
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
      title: 'Модель расшифровки',
    },
    {
      number: 2,
      type: 'summarization',
      title: 'Модель для создания сути',
    },
  ];

  const handleContinue = () => {
    goNext();
  };

  return (
    <OnboardingContainer
      title="Подготовим Memento"
      description="Выбери локальные модели для расшифровки и создания сути."
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
                        Шаг {step.number} · {step.title}

                        {step.type === "summarization" && (
                            <TooltipProvider>
                            <Tooltip>
                                <TooltipTrigger asChild>
                                <button className="text-[var(--fg3)] hover:text-[var(--fg2)]">
                                    <Info className="w-4 h-4" />
                                </button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs text-sm">
                                Позже в настройках можно выбрать внешнего провайдера, например GigaChat, OpenAI или Ollama.
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
            Продолжить
          </Button>
          <div className="text-center">
            <a
              href="https://github.com/andyzt/meet_at_giga/issues"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-[var(--fg2)] hover:underline"
            >
              Сообщить о проблеме на GitHub
            </a>
          </div>
        </div>
      </div>
    </OnboardingContainer>
  );
}
