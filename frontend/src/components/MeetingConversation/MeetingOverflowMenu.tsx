"use client";

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Copy, Languages, Loader2, MoreHorizontal, Save, Settings, Trash2 } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
  const meetingDrawer = useMeetingDrawer();
  const [open, setOpen] = useState(false);
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

  const closeMenu = () => setOpen(false);

  const handleLanguageChange = useCallback(async (code: string | null) => {
    setLanguage(code);
    try {
      const saved = await saveMeetingSummaryLanguage(meetingId, code);
      setLanguage(saved.language);
    } catch (e) {
      toast.error(t('Failed to save summary language'));
    }
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
    <div>
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

          <DropdownMenuSub>
            <DropdownMenuSubTrigger className="rounded-[9px] px-2.5 py-[9px]">
              <Languages size={16} />
              <span>{t('Summary language')}</span>
              <DropdownMenuShortcut className="tracking-normal">
                {languageLabel}
              </DropdownMenuShortcut>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="rounded-lg border-0 bg-transparent p-0 shadow-none">
              <LanguagePickerPopover
                value={language}
                onChange={handleLanguageChange}
                onClose={closeMenu}
                autoSubtitle={t('Uses dominant transcript language')}
              />
            </DropdownMenuSubContent>
          </DropdownMenuSub>

          <DropdownMenuItem
            onSelect={() => setModelOpen(true)}
            className="rounded-[9px] px-2.5 py-[9px]"
          >
            <Settings size={16} />
            <span>{t('AI Model')}</span>
            <DropdownMenuShortcut className="max-w-[96px] truncate tracking-normal">
              {modelLabel}
            </DropdownMenuShortcut>
          </DropdownMenuItem>

          <DropdownMenuSeparator className="mx-1.5 my-1" />

          <DropdownMenuItem
            disabled={deleting}
            onSelect={() => void handleDelete()}
            className="rounded-[9px] px-2.5 py-[9px] text-destructive focus:bg-destructive/10 focus:text-destructive"
          >
            {deleting ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
            <span>{t('Delete meeting')}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

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
