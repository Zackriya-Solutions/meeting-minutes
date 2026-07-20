"use client";

import { type RefObject } from 'react';
import { Loader2 } from '@/components/memento/LucideCompat';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';

/**
 * Pinned composer for the meeting conversation (variant 2a), matching the delivery
 * prototype: a single rounded "dock" (border-strong pill, bg-elevated) with an
 * auto-growing textarea and a gold circular send button. Keyboard handling
 * (Enter to send, Shift+Enter newline, ↑/↓ query history) is owned by
 * `useMeetingChat.onKeyDown`.
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
    <div className="border-t border-[var(--border-subtle)] px-[22px] pb-4 pt-3">
      <div className="mx-auto max-w-[760px]">
        <div className="flex items-end gap-2.5 rounded-[999px] border border-[var(--border-strong)] bg-[var(--bg-elevated)] py-1.5 pl-4 pr-1.5">
          <textarea
            ref={inputRef}
            value={input}
            disabled={disabled}
            onChange={(e) => onInputChange(e.target.value)}
            onKeyDown={onKeyDown}
            rows={1}
            placeholder={t('Ask about this meeting…')}
            className="max-h-40 min-h-[36px] flex-1 resize-none self-center bg-transparent py-2 text-[13.5px] leading-relaxed text-[var(--fg1)] outline-none placeholder:text-[var(--fg3)]"
          />
          <button
            type="button"
            onClick={() => onSend(input)}
            disabled={!canSend}
            aria-label={t('Send')}
            className={cn(
              'flex h-9 w-9 shrink-0 items-center justify-center self-end rounded-full transition-colors',
              canSend
                ? 'bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]'
                : 'cursor-not-allowed bg-[var(--bg-surface)] text-[var(--fg3)]',
            )}
          >
            {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Icon name="send" size={17} />}
          </button>
        </div>
      </div>
    </div>
  );
}
