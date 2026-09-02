'use client';

import { Keyboard, Mic2 } from 'lucide-react';

export type CaptureMode = 'type-anywhere' | 'record-meeting';

interface CaptureModeSwitcherProps {
  value: CaptureMode;
  onChange: (mode: CaptureMode) => void;
  meetingModeLocked?: boolean;
}

const modes: Array<{
  value: CaptureMode;
  label: string;
  description: string;
  icon: typeof Keyboard;
}> = [
  {
    value: 'type-anywhere',
    label: 'Type anywhere',
    description: 'Dictate into the app you are using',
    icon: Keyboard,
  },
  {
    value: 'record-meeting',
    label: 'Record meeting',
    description: 'Capture and review a full conversation',
    icon: Mic2,
  },
];

export function CaptureModeSwitcher({
  value,
  onChange,
  meetingModeLocked = false,
}: CaptureModeSwitcherProps) {
  return (
    <div
      role="radiogroup"
      aria-label="Capture mode"
      className="grid w-full grid-cols-1 gap-2 sm:grid-cols-2"
    >
      {modes.map((mode) => {
        const Icon = mode.icon;
        const isSelected = value === mode.value;
        const isDisabled = meetingModeLocked && mode.value !== 'record-meeting';

        return (
          <button
            key={mode.value}
            type="button"
            role="radio"
            aria-checked={isSelected}
            disabled={isDisabled}
            onClick={() => onChange(mode.value)}
            className={`group min-h-[64px] rounded-[3px] border px-3 py-3 text-left transition-[background-color,border-color,transform] duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--pt-accent)] focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${
              isSelected
                ? 'border-[var(--pt-border-hover)] bg-[var(--pt-accent-wash)]'
                : 'border-[var(--pt-border)] bg-[var(--pt-surface)] hover:border-[var(--pt-border-strong)] hover:bg-[var(--pt-surface-alt)] active:scale-[.99]'
            }`}
          >
            <span className="flex items-start gap-3">
              <span
                aria-hidden="true"
                className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-[2px] border ${
                  isSelected
                    ? 'border-[var(--pt-accent-soft)] bg-[var(--pt-surface)] text-[var(--pt-accent-active)]'
                    : 'border-[var(--pt-border)] text-[var(--pt-text-secondary)]'
                }`}
              >
                <Icon size={16} />
              </span>
              <span className="min-w-0">
                <span className="flex items-center gap-2 text-sm font-medium text-[var(--pt-text)]">
                  {mode.label}
                  {mode.value === 'type-anywhere' && (
                    <span className="pt-badge text-[9px]">Preview</span>
                  )}
                </span>
                <span className="mt-1 block text-xs leading-4 text-[var(--pt-text-tertiary)]">
                  {mode.description}
                </span>
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
