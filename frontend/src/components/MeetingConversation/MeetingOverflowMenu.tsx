"use client";

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { Copy, Languages, Loader2, MoreHorizontal, Save, Settings, Trash2 } from '@/components/deslop-icons';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { ModelSettingsModal, type ModelConfig } from '@/components/ModelSettingsModal';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { readMeetingSummaryLanguage, saveMeetingSummaryLanguage } from '@/lib/summary-language-preferences';
import { labelForCode } from '@/lib/summary-languages';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';

/**
 * The "⋯" menu for the meeting conversation (variant 2a), matching the delivery
 * prototype: Копировать саммари · Сохранить в заметку · Язык саммари · AI-модель ·
 * — · Удалить встречу (danger). Built as a plain anchored dropdown with a click-away
 * overlay (as in the prototype), not a component library menu, so the language
 * picker and model dialog compose cleanly.
 */

interface MeetingOverflowMenuProps {
  meetingId: string;
  hasSummary: boolean;
  onCopySummary: () => Promise<void> | void;
  onSaveSummary: () => Promise<void> | void;
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
}

function MenuRow({
  icon,
  label,
  right,
  onClick,
  disabled = false,
  danger = false,
}: {
  icon: React.ReactNode;
  label: string;
  right?: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'flex w-full items-center gap-2.5 rounded-[9px] px-2.5 py-[9px] text-left text-[13px] transition-colors disabled:opacity-40',
        danger
          ? 'text-destructive hover:bg-destructive/10'
          : 'text-foreground hover:bg-accent',
      )}
    >
      <span className={cn('shrink-0', danger ? 'text-destructive' : 'text-muted-foreground')}>{icon}</span>
      <span className="flex-1">{label}</span>
      {right && <span className="text-[11px] text-muted-foreground">{right}</span>}
    </button>
  );
}

export function MeetingOverflowMenu({
  meetingId,
  hasSummary,
  onCopySummary,
  onSaveSummary,
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
}: MeetingOverflowMenuProps) {
  const t = useT();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<'menu' | 'language'>('menu');
  const [modelOpen, setModelOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [language, setLanguage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let active = true;
    readMeetingSummaryLanguage(meetingId)
      .then((r) => { if (active) setLanguage(r.language); })
      .catch(() => { if (active) setLanguage(null); });
    return () => { active = false; };
  }, [open, meetingId]);

  const closeMenu = () => { setOpen(false); setView('menu'); };

  const handleLanguageChange = useCallback(async (code: string | null) => {
    setLanguage(code);
    try {
      const saved = await saveMeetingSummaryLanguage(meetingId, code);
      setLanguage(saved.language);
    } catch (e) {
      toast.error(t('Failed to save summary language'));
    }
    setView('menu');
    setOpen(false);
  }, [meetingId, t]);

  const handleDelete = useCallback(async () => {
    if (deleting) return;
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('Delete this meeting? This cannot be undone.'))) return;
    setDeleting(true);
    try {
      await invoke('api_delete_meeting', { meetingId, deleteRecordingFiles: false });
      toast.success(t('Meeting deleted'));
      closeMenu();
      router.push('/');
    } catch (e) {
      console.error('Failed to delete meeting:', e);
      toast.error(`${t('Failed to delete meeting')}: ${String(e)}`);
    } finally {
      setDeleting(false);
    }
  }, [deleting, meetingId, router, t]);

  const languageLabel = language ? labelForCode(language) : t('Auto');
  const modelLabel = modelConfig.model || modelConfig.provider || '—';

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => (open ? closeMenu() : setOpen(true))}
        aria-label={t('More actions')}
        title={t('More actions')}
        className="inline-flex h-[38px] w-[38px] items-center justify-center rounded-full border border-border bg-transparent text-foreground transition-colors hover:bg-accent"
      >
        <MoreHorizontal size={18} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-30" onClick={closeMenu} />
          <div
            className={cn(
              'absolute right-0 top-[44px] z-40 rounded-[14px] border border-border bg-muted p-1.5 shadow-[0_18px_44px_rgba(0,0,0,0.55)]',
              view === 'language' ? 'w-[300px]' : 'w-[248px]',
            )}
          >
            {view === 'menu' ? (
              <div className="flex flex-col gap-0.5">
                <MenuRow icon={<Copy size={16} />} label={t('Copy summary')} disabled={!hasSummary} onClick={() => { void onCopySummary(); closeMenu(); }} />
                <MenuRow icon={<Save size={16} />} label={t('Save to note')} disabled={!hasSummary} onClick={() => { void onSaveSummary(); closeMenu(); }} />
                <MenuRow icon={<Languages size={16} />} label={t('Summary language')} right={languageLabel} onClick={() => setView('language')} />
                <MenuRow icon={<Settings size={16} />} label={t('AI Model')} right={modelLabel} onClick={() => { setModelOpen(true); closeMenu(); }} />
                <div className="mx-1.5 my-1 h-px bg-border" />
                <MenuRow
                  icon={deleting ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
                  label={t('Delete meeting')}
                  danger
                  disabled={deleting}
                  onClick={() => void handleDelete()}
                />
              </div>
            ) : (
              <LanguagePickerPopover
                value={language}
                onChange={handleLanguageChange}
                onClose={() => setView('menu')}
                autoSubtitle={t('Uses dominant transcript language')}
              />
            )}
          </div>
        </>
      )}

      <Dialog open={modelOpen} onOpenChange={setModelOpen}>
        <DialogContent className="max-w-lg">
          <ModelSettingsModal
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            onSave={(config) => { void onSaveModelConfig(config); setModelOpen(false); }}
            layout="inline"
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}
