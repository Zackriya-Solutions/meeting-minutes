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

export function ManagedDefaultsMigrationDialog() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    invoke<Record<string, string>>('get_app_settings')
      .then((settings) => {
        if (active && settings[MIGRATION_KEY] === 'pending_confirmation') setOpen(true);
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
              {t('Your current transcription and summary models run on this device. Memento can switch these historical defaults to managed cloud providers, but only with your confirmation.')}
            </span>
            <span className="block font-medium text-[var(--fg1)]">
              {t('If you choose managed providers, meeting audio is sent to SaluteSpeech for transcription and transcript text is sent to DeepSeek for summaries. You can change this later in Settings.')}
            </span>
          </DialogDescription>
        </DialogHeader>
        {error && <p className="text-sm text-[var(--danger)]">{error}</p>}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={saving} onClick={() => void resolve(false)}>
            {t('Keep processing on this device')}
          </Button>
          <Button type="button" disabled={saving} onClick={() => void resolve(true)}>
            {saving ? t('Saving…') : t('Use managed cloud providers')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
