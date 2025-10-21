import { useCallback, RefObject } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

interface UseCopyOperationsProps {
  meeting: any;
  transcripts: Transcript[];
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
}

export function useCopyOperations({
  meeting,
  transcripts,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
}: UseCopyOperationsProps) {

  // Copy transcript to clipboard
  const handleCopyTranscript = useCallback(async () => {
    // Format timestamps as recording-relative [MM:SS] instead of wall-clock time
    const formatTime = (seconds: number | undefined, fallbackTimestamp: string): string => {
      if (seconds === undefined) {
        // For old transcripts without audio_start_time, use wall-clock time
        return fallbackTimestamp;
      }
      const totalSecs = Math.floor(seconds);
      const mins = Math.floor(totalSecs / 60);
      const secs = totalSecs % 60;
      return `[${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
    };

    const header = `# Transcript of the Meeting: ${meeting.id} - ${meetingTitle ?? meeting.title}\n\n`;
    const date = `## Date: ${new Date(meeting.created_at).toLocaleDateString()}\n\n`;
    const fullTranscript = transcripts
      .map(t => `${formatTime(t.audio_start_time, t.timestamp)} ${t.text}`)
      .join('\n');

    await navigator.clipboard.writeText(header + date + fullTranscript);
    toast.success("Transcript copied to clipboard");

    // Track copy analytics
    const wordCount = transcripts
      .map(t => t.text.split(/\s+/).length)
      .reduce((a, b) => a + b, 0);

    await Analytics.trackCopy('transcript', {
      meeting_id: meeting.id,
      transcript_length: transcripts.length.toString(),
      word_count: wordCount.toString()
    });
  }, [transcripts, meeting, meetingTitle]);

  // Copy summary to clipboard
  const handleCopySummary = useCallback(async () => {
    try {
      let summaryMarkdown = '';

      console.log('🔍 Copy Summary - Starting...');

      // Try to get markdown from BlockNote editor first
      if (blockNoteSummaryRef.current?.getMarkdown) {
        console.log('📝 Trying to get markdown from ref...');
        summaryMarkdown = await blockNoteSummaryRef.current.getMarkdown();
        console.log('📝 Got markdown from ref, length:', summaryMarkdown.length);
      }

      // Fallback: Check if aiSummary has markdown property
      if (!summaryMarkdown && aiSummary && 'markdown' in aiSummary) {
        console.log('📝 Using markdown from aiSummary');
        summaryMarkdown = (aiSummary as any).markdown || '';
        console.log('📝 Markdown from aiSummary, length:', summaryMarkdown.length);
      }

      // Fallback: Check for legacy format
      if (!summaryMarkdown && aiSummary) {
        console.log('📝 Converting legacy format to markdown');
        const sections = Object.entries(aiSummary)
          .filter(([key]) => {
            // Skip non-section keys
            return key !== 'markdown' && key !== 'summary_json' && key !== '_section_order' && key !== 'MeetingName';
          })
          .map(([, section]) => {
            if (section && typeof section === 'object' && 'title' in section && 'blocks' in section) {
              const sectionTitle = `## ${section.title}\n\n`;
              const sectionContent = section.blocks
                .map((block: any) => `- ${block.content}`)
                .join('\n');
              return sectionTitle + sectionContent;
            }
            return '';
          })
          .filter(s => s.trim())
          .join('\n\n');
        summaryMarkdown = sections;
        console.log('📝 Converted legacy format, length:', summaryMarkdown.length);
      }

      // If still no summary content, show message
      if (!summaryMarkdown.trim()) {
        console.error('❌ No summary content available to copy');
        toast.error('No summary content available to copy');
        return;
      }

      // Build metadata header
      const header = `# Meeting Summary: ${meetingTitle}\n\n`;
      const metadata = `**Meeting ID:** ${meeting.id}\n**Date:** ${new Date(meeting.created_at).toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit'
      })}\n**Copied on:** ${new Date().toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit'
      })}\n\n---\n\n`;

      const fullMarkdown = header + metadata + summaryMarkdown;
      await navigator.clipboard.writeText(fullMarkdown);

      console.log('✅ Successfully copied to clipboard!');
      toast.success("Summary copied to clipboard");

      // Track copy analytics
      await Analytics.trackCopy('summary', {
        meeting_id: meeting.id,
        has_markdown: (!!aiSummary && 'markdown' in aiSummary).toString()
      });
    } catch (error) {
      console.error('❌ Failed to copy summary:', error);
      toast.error("Failed to copy summary");
    }
  }, [aiSummary, meetingTitle, meeting, blockNoteSummaryRef]);

  return {
    handleCopyTranscript,
    handleCopySummary,
  };
}
