"use client";

import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Copy, Download, Loader2, RefreshCw, Save, Settings } from '@/components/memento/LucideCompat';
import { DetectSpeakersButton } from '@/components/MeetingDetails/DetectSpeakersButton';
import { SpeakerNameCandidatesButton } from '@/components/MeetingDetails/SpeakerNameCandidatesButton';
import { DeleteMeetingButton } from '@/components/MeetingDetails/DeleteMeetingButton';
import { RetranscribeDialog } from '@/components/MeetingDetails/RetranscribeDialog';
import { useConfig } from '@/contexts/ConfigContext';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';

/**
 * The "⋯" menu for the meeting conversation (variant 2a): gathers every secondary
 * action that used to live in the two toolbars. Summary actions, transcript
 * actions, then the destructive delete separated at the bottom. Built on a Popover
 * (not a menu) so the existing self-contained action buttons — which open their own
 * dialogs and own their Tauri command wiring — can be reused verbatim.
 */

interface MeetingOverflowMenuProps {
  meetingId: string;
  meetingFolderPath?: string | null;
  hasSummary: boolean;
  isSaving: boolean;
  isDirty: boolean;
  onCopySummary: () => Promise<void> | void;
  onSaveSummary: () => Promise<void> | void;
  onOpenModelSettings: () => void;
  speakerCount?: number;
  onSpeakersDetected?: () => Promise<void> | void;
  onRefetchTranscripts?: () => Promise<void>;
}

function MenuButton({
  icon,
  label,
  onClick,
  disabled = false,
  danger = false,
}: {
  icon: React.ReactNode;
  label: string;
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
        'flex w-full items-center gap-2.5 rounded-[var(--radius-8)] px-2.5 py-2 text-left text-sm transition-colors disabled:opacity-40',
        danger
          ? 'text-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]'
          : 'text-[var(--fg1)] hover:bg-[var(--state-hover-bg)]',
      )}
    >
      <span className="shrink-0 text-[var(--fg3)]">{icon}</span>
      {label}
    </button>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <p className="mm-eyebrow px-2.5 pb-1 pt-2">{children}</p>;
}

export function MeetingOverflowMenu({
  meetingId,
  meetingFolderPath,
  hasSummary,
  isSaving,
  isDirty,
  onCopySummary,
  onSaveSummary,
  onOpenModelSettings,
  speakerCount = 0,
  onSpeakersDetected,
  onRefetchTranscripts,
}: MeetingOverflowMenuProps) {
  const t = useT();
  const { betaFeatures } = useConfig();
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [showRetranscribe, setShowRetranscribe] = useState(false);

  const canEnhance = Boolean(betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath);

  const handleExportMp3 = useCallback(async () => {
    if (!meetingId || exporting) return;
    setExporting(true);
    try {
      const exportedPath = await invoke<string | null>('export_meeting_audio_mp3', { meetingId });
      if (exportedPath) toast.success(t('MP3 export completed'));
    } catch (error) {
      console.error('Failed to export meeting audio as MP3:', error);
      toast.error(`${t('Failed to export MP3')}: ${String(error)}`);
    } finally {
      setExporting(false);
    }
  }, [exporting, meetingId, t]);

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="mm-icon-button mm-hover"
            aria-label={t('More actions')}
            title={t('More actions')}
          >
            <span aria-hidden className="text-lg leading-none">⋯</span>
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-64 p-1.5">
          <SectionLabel>{t('Summary')}</SectionLabel>
          <MenuButton
            icon={<Copy size={16} />}
            label={t('Copy')}
            disabled={!hasSummary}
            onClick={() => {
              Analytics.trackButtonClick('copy_summary', 'meeting_details');
              void onCopySummary();
              setOpen(false);
            }}
          />
          <MenuButton
            icon={isSaving ? <Loader2 size={16} className="animate-spin" /> : <Save size={16} />}
            label={t('Save changes')}
            disabled={!hasSummary || !isDirty || isSaving}
            onClick={() => {
              void onSaveSummary();
              setOpen(false);
            }}
          />
          <MenuButton
            icon={<Settings size={16} />}
            label={t('AI Model')}
            onClick={() => {
              onOpenModelSettings();
              setOpen(false);
            }}
          />

          <div className="my-1 h-px bg-[var(--border-subtle)]" />
          <SectionLabel>{t('Transcript')}</SectionLabel>
          <div className="flex flex-col gap-1 px-0.5">
            <DetectSpeakersButton meetingId={meetingId} speakerCount={speakerCount} onDetected={onSpeakersDetected} />
            <SpeakerNameCandidatesButton meetingId={meetingId} onApplied={onSpeakersDetected} />
          </div>
          <MenuButton
            icon={exporting ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}
            label={t('Export MP3')}
            disabled={!meetingFolderPath || exporting}
            onClick={() => void handleExportMp3()}
          />
          {canEnhance && (
            <MenuButton
              icon={<RefreshCw size={16} />}
              label={t('Enhance')}
              onClick={() => {
                Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
                setShowRetranscribe(true);
                setOpen(false);
              }}
            />
          )}

          <div className="my-1 h-px bg-[var(--border-subtle)]" />
          <div className="px-0.5">
            <DeleteMeetingButton meetingId={meetingId} meetingFolderPath={meetingFolderPath} />
          </div>
        </PopoverContent>
      </Popover>

      {canEnhance && (
        <RetranscribeDialog
          open={showRetranscribe}
          onOpenChange={setShowRetranscribe}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath ?? null}
          onComplete={async () => {
            await onRefetchTranscripts?.();
          }}
        />
      )}
    </>
  );
}
