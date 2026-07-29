"use client";

import { type RefObject } from 'react';
import { Loader2 } from '@/components/deslop-icons';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';

/**
 * Pinned composer for the meeting conversation (variant 3a): a rounded dock
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
}

export function MeetingComposer({
  input,
  onInputChange,
  onKeyDown,
  onSend,
  sending,
  disabled = false,
  inputRef,
}: MeetingComposerProps) {
  const t = useT();
  const canSend = !disabled && !sending && !!input.trim();

  return (
    <div className="px-[26px] pb-[18px] pt-3">
      <div className="mx-auto max-w-[720px]">
        <div className="mm-composer flex items-end gap-2.5 rounded-[999px] border border-border bg-background py-1.5 pl-[18px] pr-2 transition-colors focus-within:border-primary/40">
          <textarea
            ref={inputRef}
            value={input}
            disabled={disabled}
            onChange={(e) => onInputChange(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder={t('Ask about this meeting…')}
            className="max-h-40 min-h-[36px] flex-1 resize-none self-center bg-transparent py-2 text-[13.5px] leading-relaxed text-foreground outline-none placeholder:text-muted-foreground"
          />
          <button
            type="button"
            onClick={() => onSend(input)}
            disabled={!canSend}
            aria-label={t('Send')}
            className={cn(
              'flex h-9 w-9 shrink-0 items-center justify-center self-end rounded-full transition-colors',
              canSend
                ? 'bg-muted text-muted-foreground hover:bg-primary hover:text-primary-foreground'
                : 'cursor-not-allowed bg-muted text-muted-foreground',
            )}
          >
            {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Icon name="send" size={17} />}
          </button>
        </div>
      </div>
    </div>
  );
}
