import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, Info } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { cn } from '@/lib/utils';
import { formatSummaryModelSizeLabelFromMb } from '@/lib/onboarding-summary-model';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface SummaryModelOption {
  name: string;
  display_name: string;
  status: {
    type: 'not_downloaded' | 'downloading' | 'available' | 'corrupted' | 'error';
  };
  size_mb: number;
  context_size: number;
  description: string;
}

export function SetupOverviewStep() {
  const {
    goNext,
    selectedSummaryModel,
    recommendedSummaryModel,
    setSelectedSummaryModel,
    setSummaryModelDownloaded,
  } = useOnboarding();
  const [isMac, setIsMac] = useState(false);
  const [summaryModels, setSummaryModels] = useState<SummaryModelOption[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);

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

  useEffect(() => {
    const loadSummaryModels = async () => {
      setModelsLoading(true);
      try {
        const models = await invoke<SummaryModelOption[]>('builtin_ai_list_models');
        setSummaryModels(models);
      } catch (error) {
        console.error('[SetupOverviewStep] Failed to load summary models:', error);
      } finally {
        setModelsLoading(false);
      }
    };

    loadSummaryModels();
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

  const handleSummaryModelSelect = (model: SummaryModelOption) => {
    setSelectedSummaryModel(model.name);
    setSummaryModelDownloaded(model.status.type === 'available');
  };

  return (
    <OnboardingContainer
      title="Setup Overview"
      description="Meetily requires that you download the Transcription & Summarization AI models for the software to work."
      step={2}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Steps Card */}
        <div className="w-full max-w-md bg-white rounded-lg border border-gray-200 p-4">
          <div className="space-y-4">
            {steps.map((step, idx) => {
              return (
                <div
                  key={step.number}
                  className={`flex items-start gap-4 p-1`}
                >
                  <div className="flex-1 ml-1">
                    <h3 className="font-medium text-gray-900 flex items-center gap-2">
                        Step {step.number} :  {step.title}

                        {step.type === "summarization" && (
                            <TooltipProvider>
                            <Tooltip>
                                <TooltipTrigger asChild>
                                <button className="text-gray-400 hover:text-gray-600">
                                    <Info className="w-4 h-4" />
                                </button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs text-sm">
                                You can also select external AI providers like OpenAI, Claude, or
                                Ollama for summary generation in settings.
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

        <div className="w-full max-w-md space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-foreground">Summary model</h3>
            {recommendedSummaryModel && (
              <span className="text-xs text-muted-foreground">Recommended: {recommendedSummaryModel}</span>
            )}
          </div>

          <div className="grid gap-2">
            {summaryModels.map((model) => {
              const isSelected = selectedSummaryModel === model.name;
              const isRecommended = recommendedSummaryModel === model.name;
              const isAvailable = model.status.type === 'available';

              return (
                <button
                  key={model.name}
                  type="button"
                  aria-pressed={isSelected}
                  onClick={() => handleSummaryModelSelect(model)}
                  className={cn(
                    'w-full rounded-lg border bg-card p-3 text-left transition-colors',
                    isSelected
                      ? 'border-primary ring-2 ring-ring'
                      : 'border-border hover:border-primary/70'
                  )}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="break-words text-sm font-semibold text-foreground">
                          {model.display_name || model.name}
                        </span>
                        {isRecommended && (
                          <span className="rounded bg-info/15 px-2 py-0.5 text-xs font-medium text-info">
                            Recommended
                          </span>
                        )}
                      </div>
                      {model.description && (
                        <p className="mt-1 text-xs leading-5 text-muted-foreground">
                          {model.description}
                        </p>
                      )}
                      <p className="mt-1 text-xs text-muted-foreground">
                        {formatSummaryModelSizeLabelFromMb(model.size_mb)} • {model.context_size} tokens
                      </p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <span
                        className={cn(
                          'text-xs font-medium',
                          isAvailable ? 'text-success' : 'text-muted-foreground'
                        )}
                      >
                        {isAvailable ? 'Ready' : 'Download'}
                      </span>
                      {isSelected && <Check className="h-4 w-4 text-primary" />}
                    </div>
                  </div>
                </button>
              );
            })}

            {modelsLoading && (
              <div className="rounded-lg border border-border bg-card p-3 text-sm text-muted-foreground">
                Loading summary models...
              </div>
            )}
          </div>
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-4">
          <Button
            onClick={handleContinue}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white"
          >
            Let's Go
          </Button>
          <div className="text-center">
            <a
              href="https://github.com/Zackriya-Solutions/meeting-minutes"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-gray-600 hover:underline"
            >
              Report issues on GitHub
            </a>
          </div>
        </div>
      </div>
    </OnboardingContainer>
  );
}
