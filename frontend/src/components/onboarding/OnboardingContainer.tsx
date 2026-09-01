import React from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ProgressIndicator } from './shared/ProgressIndicator';
import { useOnboarding } from '@/contexts/OnboardingContext';
import type { OnboardingContainerProps } from '@/types/onboarding';

export function OnboardingContainer({
  title,
  description,
  children,
  step,
  totalSteps = 5,
  stepOffset = 0,
  hideProgress = false,
  className,
  showNavigation = false,
  onNext,
  onPrevious,
  canGoNext = true,
  canGoPrevious = true,
}: OnboardingContainerProps) {
  const { goToStep, goPrevious, goNext } = useOnboarding();

  const handlePrevious = () => {
    if (onPrevious) {
      onPrevious();
    } else {
      goPrevious();
    }
  };

  const handleNext = () => {
    if (onNext) {
      onNext();
    } else {
      goNext();
    }
  };

  const handleStepClick = (s: number) => {
    goToStep(s + stepOffset);
  };

  return (
    <div className="fixed inset-0 bg-[var(--pt-bg)] flex items-center justify-center z-50 overflow-hidden">
      <div className={cn('relative w-full max-w-3xl h-full max-h-screen flex flex-col px-8 py-7', className)}>
        <div className="flex items-center justify-between mb-10">
          <div className="text-[22px] font-medium tracking-[-.06em]">pulse <span className="text-[var(--pt-accent)]">talq</span></div>
          <span className="pt-label text-[var(--pt-text-tertiary)]">Local by design</span>
        </div>
        {/* Progress Indicator with Navigation - Fixed */}
        {step && !hideProgress && (
          <div className="mb-2 relative flex-shrink-0">
            {/* Navigation Buttons */}
            {showNavigation && (
              <div className="absolute top-1/2 -translate-y-1/2 left-0 right-0 flex justify-between pointer-events-none">
                <button
                  onClick={handlePrevious}
                  disabled={!canGoPrevious || step === 1}
                  className={cn(
                    'pointer-events-auto w-8 h-8 rounded-full bg-white border border-gray-200 shadow-sm flex items-center justify-center transition-all duration-200',
                    canGoPrevious && step !== 1
                      ? 'hover:bg-gray-50 hover:shadow-md hover:scale-110 text-gray-700'
                      : 'opacity-0 cursor-not-allowed'
                  )}
                >
                  <ChevronLeft className="w-4 h-4" />
                </button>

                <button
                  onClick={handleNext}
                  disabled={!canGoNext || step === totalSteps}
                  className={cn(
                    'pointer-events-auto w-8 h-8 rounded-full bg-white border border-gray-200 shadow-sm flex items-center justify-center transition-all duration-200',
                    canGoNext && step !== totalSteps
                      ? 'hover:bg-gray-50 hover:shadow-md hover:scale-110 text-gray-700'
                      : 'opacity-0 cursor-not-allowed'
                  )}
                >
                  <ChevronRight className="w-4 h-4" />
                </button>
              </div>
            )}

            {/* Progress Indicator */}
            <ProgressIndicator current={step} total={totalSteps} onStepClick={handleStepClick} />
          </div>
        )}

        {/* Header - Fixed */}
        <div className="mb-7 text-left space-y-3 flex-shrink-0">
          <h1 className="text-[40px] leading-none font-medium tracking-[-.055em] text-[var(--pt-text)] animate-fade-in-up">{title}</h1>
          {description && (
            <p className="pt-reading text-[19px] leading-relaxed text-[var(--pt-text-secondary)] max-w-xl animate-fade-in-up delay-75">
              {description}
            </p>
          )}
        </div>

        {/* Content - Scrollable */}
        <div className="flex-1 overflow-y-auto pr-2 custom-scrollbar">
          <div className="space-y-6">{children}</div>
        </div>
      </div>
    </div>
  );
}
