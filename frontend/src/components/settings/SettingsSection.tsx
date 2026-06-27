import React from 'react';

interface SettingsSectionProps {
  title: string;
  description?: string;
  badge?: React.ReactNode;
  children: React.ReactNode;
}

export function SettingsSection({ title, description, badge, children }: SettingsSectionProps) {
  return (
    <section className="bg-card text-card-foreground rounded-lg border border-border p-6 shadow-sm">
      <div className="mb-5">
        <div className="flex items-center gap-2">
          <h3 className="text-lg font-semibold text-foreground">{title}</h3>
          {badge}
        </div>
        {description && (
          <p className="mt-2 text-sm text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="space-y-3">{children}</div>
    </section>
  );
}
