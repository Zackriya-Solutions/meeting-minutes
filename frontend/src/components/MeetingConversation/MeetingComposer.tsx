"use client";

import { type RefObject } from 'react';
import { Loader2 } from '@/components/memento/LucideCompat';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';
import { useT } from '@/lib/i18n';

/**
 * Pinned composer for the meeting conversation (variant 2a): 2–3 suggestion chips
 * above an auto-growing textarea. Keyboard handling (Enter to send, Shift+Enter for
 * a newline, ↑/↓ query history) is owned by `useMeetingChat.onKeyDown`.
 */

interface MeetingComposerProps {
  input: string;
  onInputChange: (value: string) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  onSend: (text: string) => void;
  sending: boolean;
  disabled?: boolean;
  inputRef: RefObject<HTMLTextAreaElement>;
  /** Canonical (English) suggestion strings; translated for display + sending. */
  suggestions: string[];
  showSuggestions: boolean;
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
  showSuggestions,
}: MeetingComposerProps) {
  const t = useT();
  const canSend = !disabled && !sending && !!input.trim();

  return (
    <div className="border-t border-[var(--border-subtle)] py-4">
      {showSuggestions && suggestions.length > 0 && (
        <div className="mx-auto mb-2 flex max-w-3xl flex-wrap gap-2">
          {suggestions.map((s) => (
            <button
              key={s}
              type="button"
              disabled={disabled || sending}
              onClick={() => onSend(t(s))}
              className="rounded-full border border-[var(--border-subtle)] px-3 py-1 text-xs text-[var(--fg2)] transition-colors hover:border-[var(--gold-border)] hover:bg-[var(--gold-soft)] disabled:opacity-50"
            >
              {t(s)}
            </button>
          ))}
        </div>
      )}
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <textarea
          ref={inputRef}
          value={input}
          disabled={disabled}
          onChange={(e) => onInputChange(e.target.value)}
          onKeyDown={onKeyDown}
          rows={1}
          placeholder={t('Ask about this meeting…')}
          className="mm-field max-h-40 min-h-[48px] flex-1 resize-none py-3 text-sm outline-none"
        />
        <button
          onClick={() => onSend(input)}
          disabled={!canSend}
          className={cn(
            'flex h-11 w-11 items-center justify-center rounded-xl text-[var(--fg-inverse)] transition-colors',
            canSend ? 'bg-[var(--gold)] hover:bg-[var(--gold-active)]' : 'cursor-not-allowed bg-[var(--bg-elevated)]',
          )}
          aria-label={t('Send')}
        >
          {sending ? <Loader2 className="h-5 w-5 animate-spin" /> : <Icon name="send" />}
        </button>
      </div>
      <p className="mx-auto mt-2 max-w-3xl text-center text-xs text-[var(--fg3)]">
        {t('Answers are drawn only from your recordings, with links to the sources.')}
      </p>
    </div>
  );
}
