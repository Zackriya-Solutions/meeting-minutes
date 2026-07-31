"use client";

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import {
  Copy,
  Languages,
  Loader2,
  MoreHorizontal,
  Pencil,
  Save,
  Send,
  Settings,
  Trash2,
} from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { ModelSettingsModal, type ModelConfig } from '@/components/ModelSettingsModal';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { readMeetingSummaryLanguage, saveMeetingSummaryLanguage } from '@/lib/summary-language-preferences';
import { labelForCode } from '@/lib/summary-languages';
import { useT } from '@/lib/i18n';
import { useMeetingDrawer } from '@/contexts/MeetingDrawerContext';

/**
 * The "⋯" menu for the meeting conversation. Composed from shadcn DropdownMenu
 * primitives so focus management, keyboard navigation, positioning, and dismissal
 * follow the same accessible interaction model as the rest of the application.
 *
 * The language picker and model settings open as dialogs rather than swapping the
 * menu's contents in place: a dropdown that resizes under the pointer fights the
 * primitive's focus management, and both are self-contained editors anyway.
 */

interface MeetingOverflowMenuProps {
  meetingId: string;
  hasSummary: boolean;
  onCopySummary: () => Promise<void> | void;
  onRenameMeeting: () => void;
  onSaveSummary: () => Promise<void> | void;
  /** Omitted (along with `canShareToTelegram`) when Telegram sharing is unavailable. */
  onShareSummaryToTelegram?: () => Promise<void> | void;
  canShareToTelegram?: boolean;
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
}

export function MeetingOverflowMenu({
  meetingId,
  hasSummary,
  onCopySummary,
  onRenameMeeting,
  onSaveSummary,
  onShareSummaryToTelegram,
  canShareToTelegram = false,
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
}: MeetingOverflowMenuProps) {
  const t = useT();
  const meetingDrawer = useMeetingDrawer();
  const [open, setOpen] = useState(false);
  const [modelOpen, setModelOpen] = useState(false);
  const [languageOpen, setLanguageOpen] = useState(false);
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

  const closeMenu = () => setOpen(false);

  const handleLanguageChange = useCallback(async (code: string | null) => {
    setLanguage(code);
    try {
      const saved = await saveMeetingSummaryLanguage(meetingId, code);
      setLanguage(saved.language);
    } catch {
      toast.error(t('Failed to save summary language'));
    }
    setLanguageOpen(false);
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
      meetingDrawer?.close();
    } catch (e) {
      console.error('Failed to delete meeting:', e);
      toast.error(`${t('Failed to delete meeting')}: ${String(e)}`);
    } finally {
      setDeleting(false);
    }
  }, [deleting, meetingDrawer, meetingId, t]);

  const languageLabel = language ? labelForCode(language) : t('Auto');
  const modelLabel = modelConfig.model || modelConfig.provider || '—';

  return (
    <div className="no-drag relative z-[1]">
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t('More actions')}
            title={t('More actions')}
            className="h-[38px] w-[38px] rounded-full bg-transparent shadow-none"
          >
            <MoreHorizontal size={18} />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          align="end"
          sideOffset={6}
          className="w-[248px] rounded-[14px] p-1.5"
        >
          <DropdownMenuItem
            onSelect={onRenameMeeting}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Pencil size={16} />
            <span>{t('Rename meeting')}</span>
          </DropdownMenuItem>

          <DropdownMenuSeparator className="mx-1.5 my-1" />

          <DropdownMenuItem
            disabled={!hasSummary}
            onSelect={() => void onCopySummary()}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Copy size={16} />
            <span>{t('Copy summary')}</span>
          </DropdownMenuItem>

          <DropdownMenuItem
            disabled={!hasSummary}
            onSelect={() => void onSaveSummary()}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Save size={16} />
            <span>{t('Save to note')}</span>
          </DropdownMenuItem>

          {canShareToTelegram && onShareSummaryToTelegram && (
            <DropdownMenuItem
              disabled={!hasSummary}
              onSelect={() => void onShareSummaryToTelegram()}
              className="rounded-[9px] px-2.5 py-[9px]"
            >
              <Send size={16} />
              <span>{t('Send to Telegram')}</span>
            </DropdownMenuItem>
          )}

          <DropdownMenuItem
            onSelect={() => setLanguageOpen(true)}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Languages size={16} />
            <span className="flex-1">{t('Summary language')}</span>
            <span className="text-[11px] text-[var(--fg3)]">{languageLabel}</span>
          </DropdownMenuItem>

          <DropdownMenuItem
            onSelect={() => setModelOpen(true)}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Settings size={16} />
            <span className="flex-1">{t('AI Model')}</span>
            <span className="text-[11px] text-[var(--fg3)]">{modelLabel}</span>
          </DropdownMenuItem>

          <DropdownMenuSeparator className="mx-1.5 my-1" />

          <DropdownMenuItem
            disabled={deleting}
            onSelect={(event) => {
              // Deleting is async and goes through window.confirm(); keep the menu
              // mounted so the spinner has somewhere to render.
              event.preventDefault();
              void handleDelete();
            }}
            className="rounded-[9px] px-2.5 py-[9px] text-[var(--danger)] focus:text-[var(--danger)]"
          >
            {deleting ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
            <span>{t('Delete meeting')}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={languageOpen} onOpenChange={setLanguageOpen}>
        <DialogContent className="max-w-[320px] p-1.5">
          <LanguagePickerPopover
            value={language}
            onChange={handleLanguageChange}
            onClose={() => setLanguageOpen(false)}
            autoSubtitle={t('Uses dominant transcript language')}
          />
        </DialogContent>
      </Dialog>

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
