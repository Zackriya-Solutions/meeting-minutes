import { Mic, Volume2 } from 'lucide-react';
import { TranscriptSource } from '@/types';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

interface TranscriptSourceIndicatorProps {
  source?: TranscriptSource;
}

export function TranscriptSourceIndicator({ source }: TranscriptSourceIndicatorProps) {
  if (!source) return null;

  const isMicrophone = source === 'mic';
  const Icon = isMicrophone ? Mic : Volume2;
  const label = isMicrophone ? 'Microphone' : 'System audio';

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          aria-label={label}
          className="inline-flex size-4 shrink-0 items-center justify-center text-gray-400"
        >
          <Icon aria-hidden="true" className="size-3.5" />
        </span>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
