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
import type { UseAnalyticsReportResult } from '@/hooks/meeting-details/useAnalyticsReport';
import { AnalyticsReportDialog } from './AnalyticsReportDialog';

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
  /** Report pipeline state, owned by the meeting screen so the analytics tabs share it. */
  report: UseAnalyticsReportResult;
  /**
   * True when a finished report exists for this meeting (the run the analytics tabs read).
   * Kept separate from `report.status`, which describes the LATEST run — a failed
   * regeneration must not hide the report the user already has.
   */
  canOpenReport?: boolean;
  hasSummary: boolean;
  hasTranscript: boolean;
  onCopySummary: () => Promise<void> | void;
  onRenameMeeting: () => void;
  /** Omitted (along with `canShareToTelegram`) when Telegram sharing is unavailable. */
  onShareSummaryToTelegram?: () => Promise<void> | void;
  canShareToTelegram?: boolean;
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  /** Re-runs the refinement pass (diarize → per-turn ASR → reply splitting). */
  onReprocess: () => Promise<void> | void;
  /** Stage of a pass already running, shown next to the item; null when idle. */
  reprocessingLabel?: string | null;
}

export function MeetingOverflowMenu({
  meetingId,
  report,
  canOpenReport = false,
  hasTranscript,
  onRenameMeeting,
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onReprocess,
  reprocessingLabel = null,
}: MeetingOverflowMenuProps) {
  const t = useT();
  const meetingDrawer = useMeetingDrawer();
  const [open, setOpen] = useState(false);
  const [reportDialogOpen, setReportDialogOpen] = useState(false);
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
  const reportTrailing = report.status === 'completed'
    ? t('Generate again')
    : report.status === 'running'
      ? `${report.stageIndex}/${report.totalStages}`
      : report.status === 'waiting_input'
        ? t('Answer')
        : report.status === 'failed'
          ? t('Retry')
          : undefined;
  const reportIcon = report.status === 'completed'
    ? 'refresh'
    : report.status === 'running'
      ? 'progress_activity'
      : report.status === 'waiting_input'
        ? 'help'
        : report.status === 'failed'
          ? 'error'
          : 'analytics';

  const handleAnalyticsReport = () => {
    setReportDialogOpen(true);
    if (report.status === 'idle' || report.status === 'completed' || report.status === 'failed') {
      void report.generate();
    }
  };

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
            className="no-drag h-10 w-10 rounded-full shadow-none [&>span:first-child]:!bg-[var(--primary-5)]"
          >
            <MaterialSymbol name="more_horiz" size={18} weight={400} />
          </FluidButton>
          )}
        />

        <DropdownContent
          align="end"
          sideOffset={6}
          className="meeting-overflow-menu-surface"
        >
          <MenuItem
            index={0}
            iconName="edit"
            label={t('Rename')}
            onSelect={onRenameMeeting}
          />

          <DropdownSeparator />

          {canOpenReport && (
            <MenuItem
              index={3}
              iconName="open_in_new"
              label={t('Open report')}
              onSelect={() => void report.openReport()}
            />
          )}

          <MenuItem
            index={4}
            iconName={reportIcon}
            label={t('Analytics')}
            trailing={reportTrailing}
            disabled={!hasTranscript && report.status !== 'completed'}
            onSelect={handleAnalyticsReport}
          />

          <DropdownSeparator />

          <MenuItem
            index={5}
            iconName="language"
            label={t('Results language')}
            trailing={languageLabel}
            onSelect={() => setLanguageOpen(true)}
          />

          <MenuItem
            index={6}
            iconName="settings"
            label={t('Model')}
            trailing={modelLabel}
            onSelect={() => setModelOpen(true)}
          />

          <DropdownSeparator />

          {/* The refinement pass has no other trigger: it runs once on save, and both
              its two-minute floor and the `refinement.auto` setting leave a meeting with
              no way back to per-reply rows. */}
          <MenuItem
            index={7}
            iconName={reprocessingLabel ? 'progress_activity' : 'refresh'}
            label={t('Split replies')}
            trailing={reprocessingLabel ?? undefined}
            disabled={!!reprocessingLabel}
            closeOnClick={false}
            onSelect={() => void onReprocess()}
          />

          <DropdownSeparator />

          <MenuItem
            index={8}
            iconName={deleting ? 'progress_activity' : 'delete'}
            label={t('Delete')}
            disabled={deleting}
            closeOnClick={false}
            onSelect={() => void handleDelete()}
            className="rounded-[9px] px-2.5 py-[9px] text-[var(--danger)] focus:text-[var(--danger)]"
          />
        </DropdownContent>
      </DropdownMenu>

      <AnalyticsReportDialog
        open={reportDialogOpen}
        onOpenChange={setReportDialogOpen}
        report={report}
      />

      <Dialog open={languageOpen} onOpenChange={setLanguageOpen}>
        <DialogContent className="max-w-[320px] overflow-hidden p-0 [&_[cmdk-input-wrapper]]:pr-12">
          <LanguagePickerPopover
            value={language}
            onChange={handleLanguageChange}
            autoSubtitle={t('Uses dominant transcript language')}
            className="w-full rounded-none border-0 bg-transparent"
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
