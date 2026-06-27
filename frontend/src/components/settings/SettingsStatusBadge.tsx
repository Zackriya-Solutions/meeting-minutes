import React from 'react';
import { cn } from '@/lib/utils';

type SettingsStatusBadgeTone = 'beta' | 'local' | 'provisional' | 'final' | 'fallback' | 'review' | 'unavailable';

const toneClassName: Record<SettingsStatusBadgeTone, string> = {
  beta: 'bg-yellow-100 text-yellow-800',
  local: 'bg-blue-100 text-blue-800',
  provisional: 'bg-amber-100 text-amber-800',
  final: 'bg-green-100 text-green-800',
  fallback: 'bg-orange-100 text-orange-800',
  review: 'bg-purple-100 text-purple-800',
  unavailable: 'bg-gray-100 text-gray-700',
};

interface SettingsStatusBadgeProps {
  tone: SettingsStatusBadgeTone;
  children: React.ReactNode;
}

export function SettingsStatusBadge({ tone, children }: SettingsStatusBadgeProps) {
  return (
    <span className={cn('inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium', toneClassName[tone])}>
      {children}
    </span>
  );
}
