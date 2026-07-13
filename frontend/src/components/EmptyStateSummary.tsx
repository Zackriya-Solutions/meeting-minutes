'use client';

import { motion } from 'framer-motion';
import { FileQuestion, Sparkles } from '@/components/memento/LucideCompat';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface EmptyStateSummaryProps {
  onGenerate: () => void;
  hasModel: boolean;
  isGenerating?: boolean;
}

export function EmptyStateSummary({ onGenerate, hasModel, isGenerating = false }: EmptyStateSummaryProps) {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col items-center justify-center h-full p-8 text-center"
    >
      <FileQuestion className="w-16 h-16 text-[var(--fg3)] mb-4" />
      <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">
        No Summary Generated Yet
      </h3>
      <p className="text-sm text-[var(--fg2)] mb-6 max-w-md">
        Generate an AI-powered summary of your meeting transcript to get key points, action items, and decisions.
      </p>

      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <div>
              <Button
                onClick={onGenerate}
                disabled={!hasModel || isGenerating}
                className="gap-2"
              >
                <Sparkles className="w-4 h-4" />
                {isGenerating ? 'Generating...' : 'Generate Summary'}
              </Button>
            </div>
          </TooltipTrigger>
          {!hasModel && (
            <TooltipContent>
              <p>Сначала выбери модель в настройках</p>
            </TooltipContent>
          )}
        </Tooltip>
      </TooltipProvider>

      {!hasModel && (
        <p className="text-xs text-amber-600 mt-3">
          Сначала выбери модель в настройках
        </p>
      )}
    </motion.div>
  );
}
