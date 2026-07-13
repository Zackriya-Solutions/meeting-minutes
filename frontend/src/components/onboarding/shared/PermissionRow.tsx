import React from 'react';
import { CheckCircle2, Loader2, XCircle } from '@/components/memento/LucideCompat';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import type { PermissionRowProps } from '@/types/onboarding';

export function PermissionRow({ icon, title, description, status, isPending = false, onAction }: PermissionRowProps) {
  const isAuthorized = status === 'authorized';
  const isDenied = status === 'denied';
  const isChecking = isPending;

  const getButtonText = () => {
    if (isChecking) return 'Checking...';
    if (isDenied) return 'Open Settings';
    return 'Enable';
  };

  return (
    <div
      className={cn(
        'flex items-center justify-between rounded-2xl border px-6 py-5',
        'transition-all duration-200',
        isAuthorized ? 'border-[var(--border-strong)] bg-[var(--bg-elevated)]' : isDenied ? 'border-[color-mix(in_srgb,var(--danger)_42%,transparent)] bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]' : 'bg-[var(--bg-canvas)] border-[var(--border-subtle)]'
      )}
    >
      {/* Left side: Icon + Info */}
      <div className="flex items-center gap-3 flex-1 min-w-0">
        {/* Icon */}
        <div
          className={cn(
            'flex size-10 items-center justify-center rounded-full flex-shrink-0',
            isAuthorized ? 'bg-[var(--bg-elevated)]' : isDenied ? 'bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]' : 'bg-[var(--bg-sheet)]'
          )}
        >
          <div className={cn(isAuthorized ? 'text-[var(--fg1)]' : isDenied ? 'text-[var(--danger)]' : 'text-[var(--fg2)]')}>{icon}</div>
        </div>

        {/* Title + Description */}
        <div className="min-w-0 flex-1">
          <div className="truncate font-medium text-[var(--fg1)]">{title}</div>
          <div className="text-sm text-muted-foreground">
            {isAuthorized ? (
              <span className="text-[var(--success)] flex items-center gap-1">
                <CheckCircle2 className="w-3.5 h-3.5" />
                Access Granted
              </span>
            ) : isDenied ? (
              <span className="text-[var(--danger)] flex items-center gap-1">
                <XCircle className="w-3.5 h-3.5" />
                Доступ запрещён — разреши его в настройках системы
              </span>
            ) : (
              <span>{description}</span>
            )}
          </div>
        </div>
      </div>

      {/* Right side: Action button or checkmark */}
      <div className="flex items-center gap-2 flex-shrink-0 ml-3">
        {!isAuthorized && (
          <Button
            variant={isDenied ? "destructive" : "outline"}
            size="sm"
            onClick={onAction}
            disabled={isChecking}
            className="min-w-[100px]"
          >
            {isChecking && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {getButtonText()}
          </Button>
        )}
        {isAuthorized && (
          <div className="flex size-8 items-center justify-center rounded-full bg-[color-mix(in_srgb,var(--success)_12%,transparent)]">
            <CheckCircle2 className="w-4 h-4 text-[var(--success)]" />
          </div>
        )}
      </div>
    </div>
  );
}
