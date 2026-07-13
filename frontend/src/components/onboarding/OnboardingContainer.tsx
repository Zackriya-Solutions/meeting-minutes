import React from 'react';
import { cn } from '@/lib/utils';
import { ProgressIndicator } from './shared/ProgressIndicator';
import { useOnboarding } from '@/contexts/OnboardingContext';
import type { OnboardingContainerProps } from '@/types/onboarding';
import Image from 'next/image';
import { Icon } from '@/components/memento/Icon';

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
    <div className="fixed inset-0 z-50 flex items-center justify-center overflow-hidden bg-[var(--bg-canvas)]">
      <div className={cn('flex h-full max-h-screen w-full max-w-2xl flex-col px-6 py-6', className)}>
        <div className="mb-5 flex items-center justify-center gap-3 text-xl font-semibold tracking-[-.04em]">
          <Image src="/memento-mark.svg" alt="" width={30} height={30} />
          <span>memento</span>
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
                    'pointer-events-auto w-8 h-8 rounded-full bg-[var(--bg-canvas)] border border-[var(--border-subtle)] shadow-none flex items-center justify-center transition-all duration-200',
                    canGoPrevious && step !== 1
                      ? 'hover:bg-[var(--bg-sheet)] hover:shadow-none hover:scale-110 text-[var(--fg2)]'
                      : 'opacity-0 cursor-not-allowed'
                  )}
                >
                  <Icon name="back" size={16} />
                </button>

                <button
                  onClick={handleNext}
                  disabled={!canGoNext || step === totalSteps}
                  className={cn(
                    'pointer-events-auto w-8 h-8 rounded-full bg-[var(--bg-canvas)] border border-[var(--border-subtle)] shadow-none flex items-center justify-center transition-all duration-200',
                    canGoNext && step !== totalSteps
                      ? 'hover:bg-[var(--bg-sheet)] hover:shadow-none hover:scale-110 text-[var(--fg2)]'
                      : 'opacity-0 cursor-not-allowed'
                  )}
                >
                  <Icon name="chevron-right" size={16} />
                </button>
              </div>
            )}

            {/* Progress Indicator */}
            <ProgressIndicator current={step} total={totalSteps} onStepClick={handleStepClick} />
          </div>
        )}

        {/* Header - Fixed */}
        <div className="mb-4 text-center space-y-3 flex-shrink-0">
          <h1 className="animate-fade-in-up text-[31px] font-semibold leading-[.96] tracking-[-.04em] text-[var(--fg1)]">{title}</h1>
          {description && (
            <p className="text-base text-[var(--fg2)] max-w-md mx-auto animate-fade-in-up delay-75">
              {description}
            </p>
          )}
        </div>

        {/* Content - Scrollable */}
        <div className="flex-1 overflow-y-auto pr-2">
          <div className="space-y-6">{children}</div>
        </div>
      </div>
    </div>
  );
}
