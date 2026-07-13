import React from 'react';
import { cn } from '@/lib/utils';
import type { StatusIndicatorProps } from '@/types/onboarding';

export function StatusIndicator({ status, size = 'md' }: StatusIndicatorProps) {
  const sizeClasses = {
    sm: 'w-2 h-2',
    md: 'w-3 h-3',
    lg: 'w-4 h-4',
  };

  const statusColors = {
    idle: 'bg-neutral-300',
    checking: 'bg-[var(--gold)] animate-pulse',
    success: 'bg-[var(--success)]',
    error: 'bg-[var(--danger)]',
  };

  return <span className={cn('rounded-full inline-block', sizeClasses[size], statusColors[status])} />;
}
