import React from 'react';
import { Check, Lock, Download, CheckCircle2, BrainCircuit } from '@/components/memento/LucideCompat';

interface ProgressIndicatorProps {
  current: number;
  total: number;
  onStepClick?: (step: number) => void;
}

const stepIcons = [
  Lock,         // 1. Welcome
  BrainCircuit, // 2. Setup Overview
  Download,     // 3. Download Progress
  // Step 4 (Permissions) doesn't need icon - auto-skipped on non-macOS
];

export function ProgressIndicator({ current, total, onStepClick }: ProgressIndicatorProps) {
  const visibleSteps = Array.from({ length: total }, (_, i) => i + 1);

  return (
    <div className="mb-8">
      <div className="flex items-center justify-center gap-2">
        {visibleSteps.map((step, index) => {
          const isActive = step === current;
          const isCompleted = step < current;
          const isClickable = isCompleted && onStepClick;
          const StepIcon = stepIcons[step - 1] || CheckCircle2;

          return (
            <React.Fragment key={step}>
              {/* Step Circle */}
              <button
                onClick={() => isClickable && onStepClick(step)}
                disabled={!isClickable}
                className={`relative flex items-center justify-center transition-all duration-300 ${
                  isCompleted
                    ? 'w-7 h-7 bg-[var(--success)] rounded-full'
                    : isActive
                      ? 'w-8 h-8 bg-[var(--fg3)] rounded-full'
                      : 'w-6 h-6 bg-[var(--bg-elevated)] rounded-full'
                } ${isClickable ? 'cursor-pointer hover:scale-110 hover:shadow-none' : 'cursor-default'}`}
              >
                {isCompleted ? (
                  <Check className="w-4 h-4 text-[var(--fg-inverse)]" />
                ) : (
                  <StepIcon
                    className={`transition-all duration-300 ${
                      isActive ? 'w-4 h-4 text-[var(--fg-inverse)]' : 'w-3 h-3 text-[var(--fg2)]'
                    }`}
                  />
                )}
              </button>

              {/* Connector Line */}
              {index < visibleSteps.length - 1 && (
                <div
                  className={`h-0.5 w-6 transition-all duration-300 ${
                    isCompleted ? 'bg-[var(--success)]' : 'bg-[var(--bg-elevated)]'
                  }`}
                />
              )}
            </React.Fragment>
          );
        })}
      </div>
    </div>
  );
}
