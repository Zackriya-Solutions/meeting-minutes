import React from 'react';

interface SettingsRowProps {
  title: string;
  description?: string;
  control: React.ReactNode;
  disabledReason?: string;
}

export function SettingsRow({ title, description, control, disabledReason }: SettingsRowProps) {
  return (
    <div className="flex items-center justify-between gap-6 rounded-lg border border-border p-4">
      <div className="min-w-0 flex-1">
        <div className="font-medium text-foreground">{title}</div>
        {description && (
          <div className="mt-1 text-sm text-muted-foreground">{description}</div>
        )}
        {disabledReason && (
          <div className="mt-2 text-xs text-amber-700">{disabledReason}</div>
        )}
      </div>
      <div className="flex-shrink-0">{control}</div>
    </div>
  );
}
