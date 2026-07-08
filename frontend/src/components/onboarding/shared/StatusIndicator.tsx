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
    idle: 'bg-gray-300',
    checking: 'bg-yellow-400 animate-pulse',
    success: 'bg-green-500',
    error: 'bg-red-500',
  };

  return <span className={cn('rounded-full inline-block', sizeClasses[size], statusColors[status])} />;
}
