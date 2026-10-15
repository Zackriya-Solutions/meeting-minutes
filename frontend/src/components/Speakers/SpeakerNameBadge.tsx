'use client';

import { Check, Play, X } from 'lucide-react';
import type { SpeakerNameState } from '@/lib/speakers';

interface SpeakerNameBadgeProps {
  state: SpeakerNameState;
  /** The chip's label: the name, or "Speaker N" while only a suggestion exists. */
  label: string;
  editable: boolean;
  onConfirm: () => void;
  onReject: () => void;
  /** Plays a few seconds of this speaker; no button without it. */
  onPlaySample?: () => void;
}

const ACTION = 'inline-flex h-5 w-5 items-center justify-center rounded text-gray-500 hover:bg-gray-100 hover:text-gray-900';

/** Sits next to a chip (never inside its popover trigger): the "auto" mark or the suggested
 *  name, with confirm and "Not <name>" while editing is possible, and the sample button. */
export function SpeakerNameBadge({ state, label, editable, onConfirm, onReject, onPlaySample }: SpeakerNameBadgeProps) {
  const pending = state.kind === 'auto' || state.kind === 'suggestion' ? state : null;
  if (!pending && !onPlaySample) return null;
  return (
    <span className="inline-flex items-center gap-0.5 text-xs">
      {pending?.kind === 'auto' && (
        <span
          className="rounded bg-gray-100 px-1 text-[10px] font-medium uppercase tracking-wide text-gray-500"
          title={pending.source === 'voice' ? 'Recognised by voice' : 'Found in the conversation'}
        >
          auto
        </span>
      )}
      {pending?.kind === 'suggestion' && (
        <span className="text-gray-500" title={pending.reason ?? undefined} aria-label={`${label} might be ${pending.name}`}>
          {`· ${pending.name}?`}
        </span>
      )}
      {pending && editable && (
        <>
          <button type="button" className={ACTION} aria-label={`Confirm ${pending.name}`} title={`Confirm ${pending.name}`} onClick={onConfirm}>
            <Check className="h-3 w-3" />
          </button>
          <button type="button" className={ACTION} aria-label={`Not ${pending.name}`} title={`Not ${pending.name}`} onClick={onReject}>
            <X className="h-3 w-3" />
          </button>
        </>
      )}
      {onPlaySample && (
        <button type="button" className={ACTION} aria-label={`Play a sample of ${label}`} title={`Hear a few seconds of ${label}`} onClick={onPlaySample}>
          <Play className="h-3 w-3" />
        </button>
      )}
    </span>
  );
}
