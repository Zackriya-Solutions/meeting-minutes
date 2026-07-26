import { useCallback, useEffect, RefObject } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';

import { Summary, Transcript } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import Analytics from '@/lib/analytics';
import {
  buildExportFileName,
  buildExportMarkdown,
  ExportFormat,
  ExportScope,
  FORMAT_LABELS,
  SCOPE_LABELS,
} from '@/lib/export-markdown';

interface UseExportOperationsProps {
  meeting: { id: string; title: string; created_at: string };
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
  /** Fetches every transcript row, not just the paginated view. */
  fetchAllTranscripts: (meetingId: string) => Promise<Transcript[]>;
}

export function useExportOperations({
  meeting,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
  fetchAllTranscripts,
}: UseExportOperationsProps) {
  // The first PDF render scans system fonts (~7s). Warm it up front so that
  // cost is not paid while the user waits on a save dialog.
  useEffect(() => {
    invokeTauri('api_warm_export_engine').catch((error) => {
      console.warn('Could not warm the export engine:', error);
    });
  }, []);

  const handleExport = useCallback(
    async (scope: ExportScope, format: ExportFormat) => {
      const label = `${SCOPE_LABELS[scope]} (${FORMAT_LABELS[format]})`;

      try {
        // Unsaved editor content should win over the last persisted summary.
        let summaryMarkdown = '';
        if (scope !== 'transcript' && blockNoteSummaryRef.current?.getMarkdown) {
          try {
            summaryMarkdown = await blockNoteSummaryRef.current.getMarkdown();
          } catch (error) {
            console.warn('Could not read markdown from the editor, falling back:', error);
          }
        }

        const transcripts =
          scope === 'summary' ? [] : await fetchAllTranscripts(meeting.id);

        const markdown = buildExportMarkdown({
          meeting,
          title: meetingTitle,
          scope,
          summary: aiSummary,
          summaryMarkdown,
          transcripts,
        });

        const suggestedName = buildExportFileName(
          meetingTitle || meeting.title,
          meeting.created_at,
          scope
        );

        const toastId = toast.loading(`Exporting ${label}...`);

        const path = await invokeTauri<string | null>('api_export_document', {
          markdown,
          format,
          suggestedName,
        });

        if (!path) {
          toast.dismiss(toastId);
          return;
        }

        // The path is whatever the user picked in the save dialog, so it is the
        // only reliable thing to show; "open folder" would open the meeting
        // folder instead, which is somewhere else entirely.
        toast.success(`Exported ${label}`, { id: toastId, description: path });

        await Analytics.trackFeatureUsed(`export_${scope}_${format}`);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        console.error(`Export failed (${label}):`, error);
        toast.error(`Failed to export ${label}`, { description: message });
      }
    },
    [meeting, meetingTitle, aiSummary, blockNoteSummaryRef, fetchAllTranscripts]
  );

  return { handleExport };
}
