'use client';

import { Sparkles, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface EmptyStateSummaryProps {
  onGenerate: () => void;
  hasModel: boolean;
  isGenerating?: boolean;
}

/**
 * Teaches the next action rather than announcing an absence. When no model is
 * configured the button is disabled *and* the reason is stated in text — a
 * disabled control with the explanation hidden in a tooltip is a dead end for
 * keyboard and touch users.
 */
export function EmptyStateSummary({
  onGenerate,
  hasModel,
  isGenerating = false,
}: EmptyStateSummaryProps) {
  return (
    <div className="flex min-h-[50vh] flex-col items-center justify-center px-8 text-center animate-fade-in">
      <h3 className="text-md font-medium text-ink">No summary yet</h3>
      <p className="mt-1 max-w-[46ch] text-base leading-relaxed text-ink-muted">
        Turn this transcript into key points, decisions, and action items. The
        summary runs on the model configured in Settings.
      </p>

      <Button
        onClick={onGenerate}
        disabled={!hasModel || isGenerating}
        className="mt-5 gap-2"
      >
        {isGenerating ? (
          <Loader2 className="animate-spin" aria-hidden />
        ) : (
          <Sparkles aria-hidden />
        )}
        {isGenerating ? 'Generating…' : 'Generate summary'}
      </Button>

      {!hasModel && (
        <p className="mt-2.5 text-sm text-warn-ink">
          Choose a summary model in Settings first.
        </p>
      )}
    </div>
  );
}
