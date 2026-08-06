'use client';

import { Toaster } from 'sonner';
import { useTheme } from '@/lib/theme';

/**
 * Toasts sit bottom-right, not bottom-center: bottom-center is the recording
 * transport's slot, and a stacked toast there covers the stop button mid-meeting.
 */
export function AppToaster() {
  const { resolved } = useTheme();

  return (
    <Toaster
      theme={resolved}
      position="bottom-right"
      closeButton
      offset={16}
      gap={8}
      toastOptions={{
        classNames: {
          toast:
            'bg-elevated border border-line text-ink shadow-float rounded-lg font-sans',
          title: 'text-base font-medium text-ink',
          description: 'text-sm text-ink-muted',
          actionButton: 'bg-brand text-brand-ink rounded-md',
          cancelButton: 'bg-sunken text-ink-muted rounded-md',
          closeButton:
            'bg-elevated border-line text-ink-faint hover:text-ink hover:bg-ink/5',
          error: 'border-danger/40',
          success: 'border-brand/40',
          warning: 'border-warn/40',
        },
      }}
    />
  );
}
