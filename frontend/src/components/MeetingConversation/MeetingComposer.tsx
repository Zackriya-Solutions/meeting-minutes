"use client";

import { type RefObject } from 'react';
import { useT } from '@/lib/i18n';
import { PromptInput } from '@/components/ui/prompt-input';

/**
 * Floating composer for the meeting conversation (variant 3a): a rounded dock
 * (border-strong pill, bg-sheet) with an auto-growing textarea and a neutral send
 * button that turns gold on hover — the only gold accent kept muted per 3a.
 * Keyboard handling (Enter / Shift+Enter / ↑↓ history) is owned by useMeetingChat.
 */

interface MeetingComposerProps {
  input: string;
  onInputChange: (value: string) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  onSend: (text: string) => void;
  sending: boolean;
  disabled?: boolean;
  inputRef: RefObject<HTMLTextAreaElement>;
  suggestions: readonly string[];
  showSeparator?: boolean;
}

export function MeetingComposer({
  input,
  onInputChange,
  onKeyDown,
  onSend,
  sending,
  disabled = false,
  inputRef,
  suggestions,
  showSeparator = false,
}: MeetingComposerProps) {
  const t = useT();

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 z-20 px-[var(--drawer-content-inset)] pb-[18px] pt-[18px]">
      <div
        aria-hidden="true"
        className={`absolute inset-x-0 top-0 h-px bg-[var(--primary-10)] transition-opacity duration-150 ${
          showSeparator ? 'opacity-100' : 'opacity-0'
        }`}
      />
      <div className="prompt-input-fade pointer-events-auto mx-auto max-w-[720px]">
        <PromptInput
          inputRef={inputRef}
          value={input}
          disabled={disabled}
          sending={sending}
          onValueChange={onInputChange}
          onKeyDown={onKeyDown}
          onSubmit={() => onSend(input)}
          placeholder={t('Ask about this meeting…')}
          placeholderSuggestions={suggestions}
          sendLabel={t('Send')}
        />
      </div>
    </div>
  );
}
