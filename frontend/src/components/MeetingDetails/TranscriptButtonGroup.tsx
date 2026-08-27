"use client";

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Copy, Download, FolderOpen, RefreshCw } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { useConfig } from '@/contexts/ConfigContext';

type ExportFormat = 'txt' | 'vtt';

interface ExportResult {
  bytes_written: number;
  segments_written: number;
  segments_skipped: number;
  output_path: string;
}

interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingTitle?: string;
  meetingCreatedAt?: string;
  isRecording?: boolean;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

function buildDefaultFilename(
  title: string | undefined,
  createdAt: string | undefined,
  ext: ExportFormat,
): string {
  const safeTitle = (title ?? '')
    .replace(/[\\/:*?"<>|\x00-\x1F]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  const stem = safeTitle.length > 0 ? safeTitle.slice(0, 80) : 'transcript';
  const date = createdAt ? new Date(createdAt) : new Date();
  const yyyy = date.getFullYear();
  const mm = String(date.getMonth() + 1).padStart(2, '0');
  const dd = String(date.getDate()).padStart(2, '0');
  return `${stem} - ${yyyy}-${mm}-${dd}.${ext}`;
}

export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingTitle,
  meetingCreatedAt,
  isRecording = false,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);
  const [exporting, setExporting] = useState(false);

  const handleRetranscribeComplete = useCallback(async () => {
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  const handleExport = useCallback(
    async (format: ExportFormat) => {
      if (!meetingId || transcriptCount === 0 || isRecording || exporting) return;

      try {
        setExporting(true);
        const defaultPath = buildDefaultFilename(meetingTitle, meetingCreatedAt, format);
        const filterName = format === 'txt' ? 'Plain text' : 'WebVTT subtitles';

        const chosenPath = await save({
          defaultPath,
          filters: [{ name: filterName, extensions: [format] }],
        });

        if (!chosenPath) {
          // user cancelled — silent no-op
          return;
        }

        Analytics.trackButtonClick(`export_transcript_${format}`, 'meeting_details');

        const result = await invoke<ExportResult>('api_export_transcript', {
          meetingId,
          format,
          outputPath: chosenPath,
        });

        const summary = `${result.segments_written} segment${result.segments_written === 1 ? '' : 's'} → ${format.toUpperCase()}`;
        if (result.segments_skipped > 0) {
          toast.warning(`Exported ${summary} (${result.segments_skipped} skipped: missing timing)`, {
            description: result.output_path,
          });
        } else {
          toast.success(`Exported ${summary}`, {
            description: result.output_path,
          });
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        toast.error('Export failed', { description: message });
      } finally {
        setExporting(false);
      }
    },
    [meetingId, transcriptCount, isRecording, exporting, meetingTitle, meetingCreatedAt],
  );

  const exportDisabled = transcriptCount === 0 || isRecording || exporting || !meetingId;
  const exportTitle =
    transcriptCount === 0
      ? 'No transcript to export'
      : isRecording
        ? 'Stop recording before exporting'
        : 'Export transcript';

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden lg:inline">Copy</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="xl:px-4"
          onClick={() => {
            Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
            onOpenMeetingFolder();
          }}
          title="Open Recording Folder"
        >
          <FolderOpen className="xl:mr-2" size={18} />
          <span className="hidden lg:inline">Recording</span>
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size="sm"
              variant="outline"
              className="xl:px-4"
              disabled={exportDisabled}
              title={exportTitle}
            >
              <Download className="xl:mr-2" size={18} />
              <span className="hidden lg:inline">Export</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => handleExport('txt')}>
              Plain text (.txt)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => handleExport('vtt')}>
              WebVTT (.vtt)
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 xl:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title="Retranscribe to enhance your recorded audio"
          >
            <RefreshCw className="xl:mr-2" size={18} />
            <span className="hidden lg:inline">Enhance</span>
          </Button>
        )}
      </ButtonGroup>

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}
    </div>
  );
}
