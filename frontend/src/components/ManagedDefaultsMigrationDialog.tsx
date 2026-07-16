'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

const MIGRATION_KEY = 'migration.managed_pilot_defaults.v1';

interface MigrationReport {
  transcription_changed: boolean;
  summary_changed: boolean;
}

interface PendingMigration {
  transcription: boolean;
  summary: boolean;
}

function migrationError(reason: unknown, t: (key: string) => string): string {
  const message = typeof reason === 'string' ? reason : '';
  if (message.includes('database is locked') || message.includes('code: 5')) {
    return t('Memento is finishing background database work. Please try again in a few seconds.');
  }
  if (message.includes('managed providers are unavailable')) {
    return t('Cloud services are unavailable in this build. Your local providers remain unchanged.');
  }
  return t('Failed to save provider choice.');
}

function pendingMigration(marker: string | undefined): PendingMigration | null {
  if (!marker?.startsWith('pending_confirmation:')) return null;
  const candidates = new Set(marker.slice('pending_confirmation:'.length).split(','));
  const pending = {
    transcription: candidates.has('transcription'),
    summary: candidates.has('summary'),
  };
  return pending.transcription || pending.summary ? pending : null;
}

export function ManagedDefaultsMigrationDialog() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState<PendingMigration | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    invoke<Record<string, string>>('get_app_settings')
      .then((settings) => {
        if (!active) return;
        const migration = pendingMigration(settings[MIGRATION_KEY]);
        setPending(migration);
        setOpen(migration !== null);
      })
      .catch((reason) => {
        console.warn('Could not check managed provider migration state:', reason);
      });
    return () => {
      active = false;
    };
  }, []);

  const resolve = useCallback(async (accept: boolean) => {
    setSaving(true);
    setError(null);
    try {
      const report = await invoke<MigrationReport>('resolve_managed_defaults_migration', { accept });
      setOpen(false);
      if (accept && (report.transcription_changed || report.summary_changed)) {
        window.location.reload();
      }
    } catch (reason) {
      setError(migrationError(reason, t));
    } finally {
      setSaving(false);
    }
  }, [t]);

  return (
    <Dialog open={open} onOpenChange={() => {}}>
      <DialogContent
        hideClose
        className="max-h-[calc(100vh-32px)] max-w-2xl overflow-y-auto"
        onEscapeKeyDown={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t('Choose how Memento processes meetings')}</DialogTitle>
          <DialogDescription className="space-y-3 text-left">
            <span className="block">
              {t('Your meetings are currently processed locally. You can keep this private setup or connect managed cloud services. Nothing changes until you confirm.')}
            </span>
            {pending?.transcription && (
              <span className="block font-medium text-[var(--fg1)]">
                {t('Transcription: GigaAM or Parakeet will change to SaluteSpeech. Meeting audio will be sent to Sber for transcription.')}
              </span>
            )}
            {pending?.summary && (
              <span className="block font-medium text-[var(--fg1)]">
                {t('Summaries: the local model will change to DeepSeek. Transcript text will be sent to the managed summary service.')}
              </span>
            )}
            <span className="block">
              {t('Cloud services are enabled only after Memento verifies that they are available. You can change this choice later in Settings.')}
            </span>
          </DialogDescription>
        </DialogHeader>
        {error && (
          <p role="alert" className="rounded-2xl border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-4 py-3 text-sm leading-relaxed text-[var(--danger)]">
            {error}
          </p>
        )}
        <DialogFooter className="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:space-x-0">
          <Button
            type="button"
            className="h-auto min-h-11 w-full whitespace-normal px-4 py-3 text-center leading-snug"
            disabled={saving}
            onClick={() => void resolve(false)}
          >
            {t('Keep processing locally')}
          </Button>
          <Button
            type="button"
            variant="outline"
            className="h-auto min-h-11 w-full whitespace-normal px-4 py-3 text-center leading-snug"
            disabled={saving}
            onClick={() => void resolve(true)}
          >
            {saving ? t('Checking availability…') : t('Connect managed cloud services')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
