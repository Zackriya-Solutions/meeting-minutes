import { useState, useCallback, useRef } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import {
  classifyError,
  type ExportError,
  MAX_RETRIES,
} from '@/lib/export-errors';

interface ExportResult {
  file_path: string;
  success: boolean;
  bytes_written: number;
}

export type ExportFormat = 'markdown' | 'html' | 'pdf';

/** Which parts of a meeting to include: full / summary-only / transcript-only. */
export type ExportSection = 'full' | 'summary' | 'transcript';

interface UseExportOperationsProps {
  meetingId: string;
  meetingTitle: string;
  /** Optional list of meeting IDs for batch export. Defaults to [meetingId]. */
  meetingIds?: string[];
}

const EXPORT_COMMANDS: Record<ExportFormat, { file: string; string: string; track: string }> = {
  markdown: {
    file: 'api_export_meeting_markdown',
    string: 'api_get_meeting_markdown',
    track: 'export_markdown_file',
  },
  html: {
    file: 'api_export_meeting_html',
    string: 'api_get_meeting_html',
    track: 'export_html_file',
  },
  pdf: {
    // PDF uses the HTML generator + browser print-to-PDF; no dedicated Rust command.
    file: 'api_export_meeting_html',
    string: 'api_get_meeting_html',
    track: 'export_pdf_file',
  },
};

/**
 * Hook for exporting meeting data to Markdown / HTML / PDF, with support for
 * section filtering (full / summary-only / transcript-only) and batch export of
 * multiple meetings.
 *
 * Includes a bounded retry loop for transient failures and exposes `lastError` /
 * `clearError` so callers can render an error boundary with a retry button.
 */
export function useExportOperations({ meetingId, meetingTitle, meetingIds }: UseExportOperationsProps) {
  const [isExporting, setIsExporting] = useState(false);
  const [lastError, setLastError] = useState<ExportError | null>(null);
  // Keep a ref so retry() always uses the latest params without re-creating callbacks.
  const paramsRef = useRef({ meetingId, meetingTitle, meetingIds });
  paramsRef.current = { meetingId, meetingTitle, meetingIds };

  const clearError = useCallback(() => setLastError(null), []);

  /**
   * Internal single-meeting file export with bounded retry. Returns null when the
   * user cancels or after exhausting retries.
   */
  const exportFileWithRetry = useCallback(
    async (
      format: ExportFormat,
      section: ExportSection,
      attempt = 0,
    ): Promise<ExportResult | null> => {
      const cmd = EXPORT_COMMANDS[format];
      const id = paramsRef.current.meetingId;
      try {
        Analytics.trackFeatureUsed(cmd.track);

        const result = await invokeTauri<ExportResult>(cmd.file, {
          meetingId: id,
          format: section === 'full' ? null : section,
        });

        setLastError(null);
        toast.success(`Exported to ${result.file_path}`);
        return result;
      } catch (error) {
        const classified = classifyError(error, attempt);
        // User cancellation is never retried.
        if (classified.kind === 'cancelled') {
          return null;
        }
        if (attempt < MAX_RETRIES) {
          console.warn(
            `[useExportOperations] Export(${format}) attempt ${attempt + 1} failed, retrying...`,
            error,
          );
          await new Promise((r) => setTimeout(r, 300 * (attempt + 1)));
          return exportFileWithRetry(format, section, attempt + 1);
        }
        setLastError(classified);
        console.error(`[useExportOperations] Export(${format}) error:`, error);
        const hint =
          classified.kind === 'permission'
            ? 'Check that the selected folder is writable.'
            : classified.kind === 'not_found'
              ? 'The meeting may have been deleted.'
              : 'Please try again.';
        toast.error(`Export failed: ${classified.message}. ${hint}`);
        return null;
      }
    },
    [],
  );

  const withExporting = useCallback(
    async <T,>(fn: () => Promise<T>): Promise<T | null> => {
      setIsExporting(true);
      setLastError(null);
      try {
        return await fn();
      } finally {
        setIsExporting(false);
      }
    },
    [],
  );

  /** Export a single meeting to a Markdown file via save dialog. */
  const exportToFile = useCallback(
    (section: ExportSection = 'full') =>
      withExporting(() => exportFileWithRetry('markdown', section, 0)),
    [withExporting, exportFileWithRetry],
  );

  /** Export a single meeting to an HTML file via save dialog. */
  const exportToHtml = useCallback(
    (section: ExportSection = 'full') =>
      withExporting(() => exportFileWithRetry('html', section, 0)),
    [withExporting, exportFileWithRetry],
  );

  /**
   * Export a meeting to PDF via the browser print dialog.
   * Fetches the rendered HTML, opens a print window, and triggers print so the
   * user can "Save as PDF". Falls back to saving .html if the print window
   * cannot be opened (e.g. popup blocked).
   */
  const exportToPdf = useCallback(async (section: ExportSection = 'full'): Promise<ExportResult | null> => {
    return withExporting(async () => {
      Analytics.trackFeatureUsed('export_pdf_file');
      const html = await invokeTauri<string>('api_get_meeting_html', {
        meetingId: paramsRef.current.meetingId,
        format: section === 'full' ? null : section,
      });

      const printWindow = window.open('', '_blank', 'width=800,height=1000');
      if (!printWindow) {
        toast.error('Pop-up blocked. Allow pop-ups to export PDF.');
        return null;
      }
      printWindow.document.open();
      printWindow.document.write(html);
      printWindow.document.close();
      // Wait for layout, then open the print dialog.
      await new Promise((r) => setTimeout(r, 400));
      printWindow.focus();
      printWindow.print();
      return {
        file_path: `${paramsRef.current.meetingTitle || 'meeting'}.pdf`,
        success: true,
        bytes_written: html.length,
      };
    });
  }, [withExporting]);

  /** Batch export: combine multiple meetings into one Markdown file. */
  const exportBatchToFile = useCallback(
    (ids: string[], section: ExportSection = 'full') =>
      withExporting(async () => {
        Analytics.trackFeatureUsed('export_markdown_batch');
        const result = await invokeTauri<ExportResult>('api_export_meetings_markdown', {
          meetingIds: ids,
          format: section === 'full' ? null : section,
        });
        setLastError(null);
        toast.success(`Exported ${ids.length} meetings to ${result.file_path}`);
        return result;
      }),
    [withExporting],
  );

  /** Batch export: combine multiple meetings into one HTML file. */
  const exportBatchToHtml = useCallback(
    (ids: string[], section: ExportSection = 'full') =>
      withExporting(async () => {
        Analytics.trackFeatureUsed('export_html_batch');
        const result = await invokeTauri<ExportResult>('api_export_meetings_html', {
          meetingIds: ids,
          format: section === 'full' ? null : section,
        });
        setLastError(null);
        toast.success(`Exported ${ids.length} meetings to ${result.file_path}`);
        return result;
      }),
    [withExporting],
  );

  /** Get combined markdown for the given meeting IDs (default: current meeting). */
  const getBatchMarkdown = useCallback(
    async (ids: string[], section: ExportSection = 'full') =>
      invokeTauri<string>('api_get_meetings_markdown', {
        meetingIds: ids,
        format: section === 'full' ? null : section,
      }),
    [],
  );

  /** Get combined HTML for the given meeting IDs (default: current meeting). */
  const getBatchHtml = useCallback(
    async (ids: string[], section: ExportSection = 'full') =>
      invokeTauri<string>('api_get_meetings_html', {
        meetingIds: ids,
        format: section === 'full' ? null : section,
      }),
    [],
  );

  /** Batch export: combine ALL meetings in the app into one Markdown file. */
  const exportAllToFile = useCallback(
    (section: ExportSection = 'full') =>
      withExporting(async () => {
        const meetings = await invokeTauri<Array<{ id: string }>>('api_get_meetings');
        const ids = meetings.map((m) => m.id);
        if (ids.length === 0) {
          toast.error('No meetings to export');
          return null;
        }
        Analytics.trackFeatureUsed('export_markdown_batch_all');
        const result = await invokeTauri<ExportResult>('api_export_meetings_markdown', {
          meetingIds: ids,
          format: section === 'full' ? null : section,
        });
        setLastError(null);
        toast.success(`Exported ${ids.length} meetings to ${result.file_path}`);
        return result;
      }),
    [withExporting],
  );

  /** Batch export: combine ALL meetings in the app into one HTML file. */
  const exportAllToHtml = useCallback(
    (section: ExportSection = 'full') =>
      withExporting(async () => {
        const meetings = await invokeTauri<Array<{ id: string }>>('api_get_meetings');
        const ids = meetings.map((m) => m.id);
        if (ids.length === 0) {
          toast.error('No meetings to export');
          return null;
        }
        Analytics.trackFeatureUsed('export_html_batch_all');
        const result = await invokeTauri<ExportResult>('api_export_meetings_html', {
          meetingIds: ids,
          format: section === 'full' ? null : section,
        });
        setLastError(null);
        toast.success(`Exported ${ids.length} meetings to ${result.file_path}`);
        return result;
      }),
    [withExporting],
  );

  /** Retry the last failed file export (markdown by default). */
  const retryExport = useCallback(async (): Promise<ExportResult | null> => {
    if (!lastError || lastError.kind === 'cancelled') return null;
    return exportToFile();
  }, [lastError, exportToFile]);

  /** Copy full meeting markdown to clipboard. */
  const copyMarkdown = useCallback(async (section: ExportSection = 'full') => {
    try {
      Analytics.trackFeatureUsed('export_markdown_clipboard');
      const markdown = await invokeTauri<string>('api_get_meeting_markdown', {
        meetingId: paramsRef.current.meetingId,
        format: section === 'full' ? null : section,
      });
      await navigator.clipboard.writeText(markdown);
      setLastError(null);
      toast.success('Markdown copied to clipboard');
    } catch (error) {
      const classified = classifyError(error, 0);
      if (classified.kind === 'cancelled') return;
      setLastError(classified);
      console.error('[useExportOperations] Copy error:', error);
      toast.error(`Copy failed: ${classified.message}`);
    }
  }, []);

  return {
    isExporting,
    exportToFile,
    exportToHtml,
    exportToPdf,
    exportBatchToFile,
    exportBatchToHtml,
    exportAllToFile,
    exportAllToHtml,
    getBatchMarkdown,
    getBatchHtml,
    copyMarkdown,
    retryExport,
    lastError,
    clearError,
  };
}
