'use client';

import { Monitor, Moon, Sun } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useTheme, type Theme } from '@/lib/theme';
import { cn } from '@/lib/utils';

const OPTIONS: { value: Theme; label: string; Icon: typeof Sun }[] = [
  { value: 'light', label: 'Light', Icon: Sun },
  { value: 'dark', label: 'Dark', Icon: Moon },
  { value: 'system', label: 'System', Icon: Monitor },
];

/** Segmented control. Used in Settings, where there is room to name the options. */
export function ThemeToggle({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();

  return (
    <div
      role="radiogroup"
      aria-label="Color theme"
      className={cn(
        'inline-flex items-center gap-0.5 rounded-md border border-line bg-sunken p-0.5',
        className
      )}
    >
      {OPTIONS.map(({ value, label, Icon }) => {
        const active = theme === value;
        return (
          <button
            key={value}
            role="radio"
            aria-checked={active}
            onClick={() => setTheme(value)}
            className={cn(
              'flex items-center gap-1.5 rounded-sm px-2.5 py-1.5 text-sm font-medium',
              'transition-colors duration-fast',
              active
                ? 'bg-elevated text-ink shadow-pop'
                : 'text-ink-muted hover:text-ink'
            )}
          >
            <Icon className="h-3.5 w-3.5" aria-hidden />
            {label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * Icon button that cycles light → dark → system. Used in the rail, where a
 * segmented control does not fit. Announces the *current* theme, not the next
 * one, so screen-reader users can read state without activating it.
 */
export function ThemeToggleButton({
  showLabel = false,
  className,
}: {
  showLabel?: boolean;
  className?: string;
}) {
  const { theme, setTheme } = useTheme();
  const current = OPTIONS.find((o) => o.value === theme) ?? OPTIONS[2];
  const next = OPTIONS[(OPTIONS.indexOf(current) + 1) % OPTIONS.length];
  const { Icon } = current;

  const button = (
    <button
      onClick={() => setTheme(next.value)}
      aria-label={`Theme: ${current.label}. Switch to ${next.label}.`}
      className={cn(
        'flex items-center rounded-md text-ink-muted transition-colors duration-fast',
        'hover:bg-ink/5 hover:text-ink active:bg-ink/10',
        showLabel ? 'h-8 w-full gap-2 px-2 text-sm' : 'h-8 w-8 justify-center',
        className
      )}
    >
      <Icon className="h-4 w-4 shrink-0" aria-hidden />
      {showLabel && <span>{current.label}</span>}
    </button>
  );

  if (showLabel) return button;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">Theme: {current.label}</TooltipContent>
    </Tooltip>
  );
}
