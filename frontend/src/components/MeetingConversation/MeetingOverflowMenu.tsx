"use client";

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { MaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react';
import {
  DropdownContent,
  DropdownMenu,
  DropdownSeparator,
  DropdownTrigger,
} from '@/components/ui/dropdown';
import { MenuItem } from '@/components/ui/menu-item';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { ModelSettingsModal, type ModelConfig } from '@/components/ModelSettingsModal';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { readMeetingSummaryLanguage, saveMeetingSummaryLanguage } from '@/lib/summary-language-preferences';
import { labelForCode } from '@/lib/summary-languages';
import { useT } from '@/lib/i18n';
import { useMeetingDrawer } from '@/contexts/MeetingDrawerContext';

/**
 * The "⋯" menu for the meeting conversation. Composed from Fluid Functionalism's
 * Dropdown so proximity hover and spring motion match the rest of the interface.
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
        <DropdownTrigger
          render={(
          <FluidButton
            type="button"
            variant="secondary"
            size="icon"
            active={open}
            aria-label={t('More actions')}
            title={t('More actions')}
            data-no-window-drag
            className="no-drag h-[38px] w-[38px] rounded-full shadow-none [&>span:first-child]:!bg-[var(--primary-5)]"
          >
            <MaterialSymbol name="more_horiz" size={18} weight={400} />
          </FluidButton>
          )}
        />

        <DropdownContent
          align="end"
          sideOffset={6}
          className="w-[248px]"
        >
          <MenuItem
            index={0}
            iconName="edit"
            label={t('Rename meeting')}
            onSelect={onRenameMeeting}
          />

          <DropdownSeparator />

          <MenuItem
            index={1}
            iconName="content_copy"
            label={t('Copy summary')}
            disabled={!hasSummary}
            onSelect={() => void onCopySummary()}
          />

          <MenuItem
            index={2}
            iconName="save"
            label={t('Save to note')}
            disabled={!hasSummary}
            onSelect={() => void onSaveSummary()}
          />

          {canShareToTelegram && onShareSummaryToTelegram && (
            <MenuItem
              index={3}
              iconName="send"
              label={t('Send to Telegram')}
              disabled={!hasSummary}
              onSelect={() => void onShareSummaryToTelegram()}
            />
          )}

          <MenuItem
            index={4}
            iconName="language"
            label={t('Summary language')}
            trailing={languageLabel}
            onSelect={() => setLanguageOpen(true)}
          />

          <MenuItem
            index={5}
            iconName="settings"
            label={t('AI Model')}
            trailing={modelLabel}
            onSelect={() => setModelOpen(true)}
          />

          <DropdownSeparator />

          <MenuItem
            index={6}
            iconName={deleting ? 'progress_activity' : 'delete'}
            label={t('Delete meeting')}
            disabled={deleting}
            closeOnClick={false}
            onSelect={() => void handleDelete()}
            className="rounded-[9px] px-2.5 py-[9px] text-[var(--danger)] focus:text-[var(--danger)]"
          />
        </DropdownContent>
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
