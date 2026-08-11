import { useState, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

interface ExportResult {
  file_path: string;
  success: boolean;
  bytes_written: number;
}

interface UseExportOperationsProps {
  meetingId: string;
  meetingTitle: string;
}

/**
 * Hook for exporting meeting data to Markdown files.
 *
 * Provides two export modes:
 * 1. `exportToFile` — Shows save dialog, writes .md file with transcript + summary
 * 2. `copyMarkdown` — Copies full markdown to clipboard (all transcripts + summary)
 */
export function useExportOperations({ meetingId, meetingTitle }: UseExportOperationsProps) {
  const [isExporting, setIsExporting] = useState(false);

  /**
   * Export meeting to a Markdown file via save dialog.
   */
  const exportToFile = useCallback(async (): Promise<ExportResult | null> => {
    setIsExporting(true);
    try {
      Analytics.trackFeatureUsed('export_markdown_file');

      const result = await invokeTauri<ExportResult>('api_export_meeting_markdown', {
        meetingId,
      });

      toast.success(`Exported to ${result.file_path}`);
      return result;
    } catch (error) {
      // User cancelled dialog is not an error
      const msg = String(error);
      if (msg.includes('cancelled')) {
        return null;
      }
      console.error('[useExportOperations] Export error:', error);
      toast.error(`Export failed: ${msg}`);
      return null;
    } finally {
      setIsExporting(false);
    }
  }, [meetingId]);

  /**
   * Copy full meeting markdown to clipboard.
   */
  const copyMarkdown = useCallback(async () => {
    try {
      Analytics.trackFeatureUsed('export_markdown_clipboard');

      const markdown = await invokeTauri<string>('api_get_meeting_markdown', {
        meetingId,
      });

      await navigator.clipboard.writeText(markdown);
      toast.success('Markdown copied to clipboard');
    } catch (error) {
      console.error('[useExportOperations] Copy error:', error);
      toast.error(`Copy failed: ${String(error)}`);
    }
  }, [meetingId]);

  return {
    isExporting,
    exportToFile,
    copyMarkdown,
  };
}
