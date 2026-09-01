import { useCallback, useState, type RefObject } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import type { jsPDF } from 'jspdf';
import type { Summary, Transcript, TranscriptAnnotation } from '@/types';
import type { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import {
  buildMeetingMarkdown,
  getExportImageDataUrl,
  type ExportAnnotation,
} from '@/lib/meeting-export';

interface UseMeetingExportProps {
  meeting: { id: string; title: string; created_at: string };
  meetingTitle: string;
  aiSummary: Summary | null;
  summaryRef: RefObject<BlockNoteSummaryViewRef>;
  annotations: TranscriptAnnotation[];
  getAnnotationImage?: (annotation: TranscriptAnnotation) => Promise<string | null>;
}

interface TranscriptPage {
  transcripts: Transcript[];
  total_count: number;
}

function safeFileName(value: string): string {
  return (value || 'meeting-export').replace(/[\\/:*?"<>|]/g, '-').trim() || 'meeting-export';
}

function withExtension(filePath: string, extension: string): string {
  return filePath.toLowerCase().endsWith(`.${extension}`) ? filePath : `${filePath}.${extension}`;
}

function legacySummaryToMarkdown(summary: Summary): string {
  return Object.entries(summary)
    .filter(([key, section]) =>
      !['markdown', 'summary_json', '_section_order', 'MeetingName'].includes(key) &&
      section && typeof section === 'object' && 'title' in section && 'blocks' in section
    )
    .map(([, section]) => {
      const typedSection = section as { title?: string; blocks?: Array<{ content?: string; type?: string }> };
      const content = (typedSection.blocks || []).map(block => {
        if (block.type === 'heading1') return `### ${block.content || ''}`;
        if (block.type === 'heading2') return `#### ${block.content || ''}`;
        if (block.type === 'bullet') return `- ${block.content || ''}`;
        return block.content || '';
      }).join('\n');
      return `## ${typedSection.title || 'Summary'}\n\n${content}`;
    })
    .filter(section => section.trim())
    .join('\n\n');
}

function markdownText(line: string): string {
  return line
    .replace(/^#{1,6}\s+/, '')
    .replace(/^>\s?/, '')
    .replace(/^[-*+]\s+/, '• ')
    .replace(/!\[.*\]\(.*\)/, '')
    .replace(/\*\*(.*?)\*\*/g, '$1')
    .replace(/__(.*?)__/g, '$1')
    .replace(/`([^`]+)`/g, '$1');
}

function addPdfText(doc: jsPDF, text: string, state: { y: number }, size: number, bold: boolean) {
  const pageWidth = doc.internal.pageSize.getWidth();
  const pageHeight = doc.internal.pageSize.getHeight();
  const margin = 40;
  const lineHeight = size * 1.35;
  const lines = doc.splitTextToSize(text, pageWidth - margin * 2);

  doc.setFont('helvetica', bold ? 'bold' : 'normal');
  doc.setFontSize(size);
  for (const line of lines) {
    if (state.y + lineHeight > pageHeight - margin) {
      doc.addPage();
      state.y = margin;
    }
    doc.text(line, margin, state.y);
    state.y += lineHeight;
  }
}

async function renderMarkdownPdf(markdown: string): Promise<Uint8Array> {
  const { jsPDF } = await import('jspdf');
  const doc = new jsPDF({ unit: 'pt', format: 'a4' });
  const state = { y: 48 };
  const margin = 40;
  const pageWidth = doc.internal.pageSize.getWidth();
  const pageHeight = doc.internal.pageSize.getHeight();

  for (const line of markdown.split('\n')) {
    if (!line.trim()) {
      state.y += 8;
      continue;
    }

    const imageStart = line.indexOf('![');
    const imageEnd = line.indexOf('](', imageStart + 2);
    if (imageStart === 0 && imageEnd > 0 && line.endsWith(')')) {
      const dataUrl = line.slice(imageEnd + 2, -1);
      try {
        const properties = doc.getImageProperties(dataUrl);
        const maxWidth = pageWidth - margin * 2;
        const maxHeight = pageHeight - margin * 2 - 24;
        const ratio = properties.width / properties.height || 1;
        const width = Math.min(maxWidth, maxHeight * ratio);
        const height = width / ratio;
        if (state.y + height > pageHeight - margin) {
          doc.addPage();
          state.y = margin;
        }
        const format = dataUrl.slice(5, dataUrl.indexOf(';')).split('/')[1].toUpperCase();
        doc.addImage(dataUrl, format === 'JPG' ? 'JPEG' : format, margin, state.y, width, height);
        state.y += height + 12;
        continue;
      } catch (error) {
        console.warn('Failed to add an exported annotation image to PDF:', error);
      }
    }

    const heading = line.match(/^(#{1,3})\s+/);
    addPdfText(doc, markdownText(line), state, heading ? (heading[1].length === 1 ? 18 : 14) : 10, Boolean(heading));
  }

  return new Uint8Array(doc.output('arraybuffer'));
}

export function useMeetingExport({
  meeting,
  meetingTitle,
  aiSummary,
  summaryRef,
  annotations,
  getAnnotationImage,
}: UseMeetingExportProps) {
  const [isExporting, setIsExporting] = useState(false);

  const fetchAllTranscripts = useCallback(async (): Promise<Transcript[]> => {
    const firstPage = await invoke<TranscriptPage>('api_get_meeting_transcripts', {
      meetingId: meeting.id,
      limit: 1,
      offset: 0,
    });
    if (firstPage.total_count === 0) return [];
    const allData = await invoke<TranscriptPage>('api_get_meeting_transcripts', {
      meetingId: meeting.id,
      limit: firstPage.total_count,
      offset: 0,
    });
    return allData.transcripts;
  }, [meeting.id]);

  const getSummaryMarkdown = useCallback(async (): Promise<string> => {
    if (summaryRef.current?.getMarkdown) {
      const editorMarkdown = await summaryRef.current.getMarkdown();
      if (editorMarkdown.trim()) return editorMarkdown;
    }
    if (!aiSummary) return '';
    if ('markdown' in aiSummary && typeof (aiSummary as Summary & { markdown?: unknown }).markdown === 'string') {
      return (aiSummary as Summary & { markdown: string }).markdown;
    }
    return legacySummaryToMarkdown(aiSummary);
  }, [aiSummary, summaryRef]);

  const prepareExport = useCallback(async () => {
    const [transcripts, summaryMarkdown] = await Promise.all([
      fetchAllTranscripts(),
      getSummaryMarkdown(),
    ]);
    const resolvedAnnotations: ExportAnnotation[] = await Promise.all(annotations.map(async annotation => {
      if (annotation.type !== 'image') return annotation;
      const dataUrl = getExportImageDataUrl(annotation);
      if (dataUrl) return { ...annotation, imageDataUrl: dataUrl };
      if (annotation.imageFile && getAnnotationImage) {
        return { ...annotation, imageDataUrl: await getAnnotationImage(annotation) || undefined };
      }
      return annotation;
    }));
    const markdown = buildMeetingMarkdown({
      title: meetingTitle || meeting.title,
      date: new Date(meeting.created_at).toLocaleDateString(),
      summaryMarkdown,
      segments: transcripts,
      annotations: resolvedAnnotations,
    });
    return { markdown, resolvedAnnotations };
  }, [annotations, fetchAllTranscripts, getAnnotationImage, getSummaryMarkdown, meeting.created_at, meeting.title, meetingTitle]);

  const runExport = useCallback(async (operation: () => Promise<void>, failureMessage: string) => {
    setIsExporting(true);
    try {
      await operation();
    } catch (error) {
      console.error(failureMessage, error);
      toast.error(failureMessage);
    } finally {
      setIsExporting(false);
    }
  }, []);

  const copyAsMarkdown = useCallback(() => runExport(async () => {
    const { markdown } = await prepareExport();
    await navigator.clipboard.writeText(markdown);
    toast.success('Meeting Markdown copied to clipboard');
  }, 'Failed to copy meeting Markdown'), [prepareExport, runExport]);

  const exportMarkdown = useCallback(() => runExport(async () => {
    const destination = await save({
      defaultPath: `${safeFileName(meetingTitle || meeting.title)}.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (!destination) return;
    const { markdown } = await prepareExport();
    await writeFile(withExtension(destination, 'md'), new TextEncoder().encode(markdown));
    toast.success('Meeting exported as Markdown');
  }, 'Failed to export meeting Markdown'), [meeting.title, meetingTitle, prepareExport, runExport]);

  const exportPdf = useCallback(() => runExport(async () => {
    const destination = await save({
      defaultPath: `${safeFileName(meetingTitle || meeting.title)}.pdf`,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    });
    if (!destination) return;
    const { markdown } = await prepareExport();
    await writeFile(withExtension(destination, 'pdf'), await renderMarkdownPdf(markdown));
    toast.success('Meeting exported as PDF');
  }, 'Failed to export meeting PDF'), [meeting.title, meetingTitle, prepareExport, runExport]);

  return { copyAsMarkdown, exportMarkdown, exportPdf, isExporting };
}
