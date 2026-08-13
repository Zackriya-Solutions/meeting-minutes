'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { createPortal } from 'react-dom';
import { Button } from '@/components/ui/button';
import { Icon } from '@/components/memento/Icon';
import { cn } from '@/lib/utils';

export type MeetingDetectionBannerState = 'suggestion' | 'starting';

interface MeetingDetectionBannerProps {
  open: boolean;
  state: MeetingDetectionBannerState;
  appNames?: string[];
  onPrimaryAction: () => void;
  onDismiss: () => void;
}

const copyByState: Record<MeetingDetectionBannerState, {
  action: string;
  detail: string;
}> = {
  suggestion: {
    action: 'Начать запись',
    detail: 'Memento готов к записи',
  },
  starting: {
    action: 'Запускаем запись…',
    detail: 'Подключаем микрофон',
  },
};

export function MeetingDetectionBanner({
  open,
  state,
  appNames = [],
  onPrimaryAction,
  onDismiss,
}: MeetingDetectionBannerProps) {
  if (typeof document === 'undefined') return null;

  const copy = copyByState[state];
  const appLabel = appNames.join(', ');
  const title = appLabel ? `Встреча в ${appLabel}` : 'Встреча началась';
  const subtitle = state === 'starting'
    ? 'Встреча обнаружена'
    : 'Обнаружен активный звонок';

  return createPortal(
    <AnimatePresence>
      {open && (
        <div className="pointer-events-none fixed inset-x-0 top-4 z-[120] flex justify-center px-4">
          <motion.section
            aria-label="Обнаружена встреча"
            aria-live="polite"
            initial={{ opacity: 0, y: -24, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -16, scale: 0.985 }}
            transition={{ type: 'spring', stiffness: 460, damping: 38, mass: 0.8 }}
            className="pointer-events-auto flex min-h-[96px] w-full max-w-[760px] items-stretch overflow-hidden rounded-[24px] border border-border bg-[var(--elevation-1)] text-foreground shadow-[0_18px_48px_rgba(0,0,0,0.18)]"
          >
            <div className="flex min-w-0 flex-1 items-center gap-5 px-5 py-4 sm:px-6">
              <span className="h-14 w-1.5 shrink-0 rounded-full bg-[#ff5d4e]" />
              <div className="min-w-0">
                <h2 className="truncate text-[20px] font-medium leading-tight tracking-[-0.02em] sm:text-[22px]">
                  {title}
                </h2>
                <p className="mt-1 truncate text-[16px] leading-tight text-muted-foreground sm:text-[18px]">
                  {subtitle}
                </p>
              </div>
            </div>

            <div className="my-4 hidden w-px bg-border sm:block" />

            <div className="hidden shrink-0 items-stretch sm:flex">
              <Button
                type="button"
                variant="ghost"
                disabled={state === 'starting'}
                onClick={onPrimaryAction}
                className={cn(
                  'h-auto rounded-none px-5 text-left hover:bg-primary/5',
                  state === 'starting' && 'opacity-70',
                )}
              >
                <span className="flex size-10 items-center justify-center rounded-xl bg-primary text-primary-foreground">
                  <Icon name="mic" size={21} />
                </span>
                <span>
                  <span className="block whitespace-nowrap text-[17px] font-medium leading-tight">
                    {copy.action}
                  </span>
                  <span className="mt-0.5 block whitespace-nowrap text-[15px] font-normal leading-tight text-muted-foreground">
                    {copy.detail}
                  </span>
                </span>
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label="Скрыть плашку"
                onClick={onDismiss}
                className="h-auto w-14 rounded-none border-l border-border text-muted-foreground hover:bg-primary/5 hover:text-foreground"
              >
                <Icon name="chevron-up" size={22} />
              </Button>
            </div>

            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Начать запись"
              disabled={state === 'starting'}
              onClick={onPrimaryAction}
              className="my-auto mr-3 size-11 shrink-0 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 hover:text-primary-foreground sm:hidden"
            >
              <Icon name="mic" size={20} />
            </Button>
          </motion.section>
        </div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
