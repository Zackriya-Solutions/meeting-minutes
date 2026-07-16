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
      setError(typeof reason === 'string' ? reason : t('Failed to save provider choice.'));
    } finally {
      setSaving(false);
    }
  }, [t]);

  return (
    <Dialog open={open} onOpenChange={(next) => !saving && setOpen(next)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('Choose where meeting processing runs')}</DialogTitle>
          <DialogDescription className="space-y-3 text-left">
            <span className="block">
              {t('Memento found historical local provider defaults that can be replaced with managed cloud providers. Nothing changes until you confirm.')}
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
              {t('Providers that do not match these historical defaults stay unchanged. You can change this choice later in Settings.')}
            </span>
          </DialogDescription>
        </DialogHeader>
        {error && <p className="text-sm text-[var(--danger)]">{error}</p>}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={saving} onClick={() => void resolve(false)}>
            {t('Keep current providers')}
          </Button>
          <Button type="button" disabled={saving} onClick={() => void resolve(true)}>
            {saving ? t('Saving…') : t('Apply managed provider changes')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
