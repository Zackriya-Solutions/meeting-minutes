'use client';

import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import type { Transcript } from '@/types';
import { toast } from 'sonner';
import { ArrowLeft, Sparkles, Download } from 'lucide-react';
import { useRouter } from 'next/navigation';

interface MeetingTranscriptsResponse {
  transcripts: Transcript[];
  total_count: number;
  has_more: boolean;
}

interface ProcessTranscriptResponse {
  process_id: string;
  message: string;
}

interface SummaryStatusResponse {
  status: string;
  data?: {
    markdown?: string;
  };
  error?: string;
}

function inferSkillName(markdown: string): string {
  const lines = markdown.split('\n');

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.toLowerCase().startsWith('name:')) {
      const value = trimmed.slice(5).trim();
      if (value) return value;
    }
  }

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('# ')) {
      return trimmed.slice(2).trim();
    }
  }

  return '';
}

function sanitizeDisplayName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/\s+/g, '-')
    .replace(/[^a-z0-9_-]/g, '');
}

export default function SkillsPage() {
  const router = useRouter();
  const { meetings } = useSidebar();
  const { modelConfig } = useConfig();

  const [selectedMeetingId, setSelectedMeetingId] = useState<string>('');
  const [skillName, setSkillName] = useState<string>('');
  const [markdown, setMarkdown] = useState<string>('');
  const [isGenerating, setIsGenerating] = useState(false);
  const [isExporting, setIsExporting] = useState(false);

  const selectedMeeting = useMemo(
    () => meetings.find((meeting) => meeting.id === selectedMeetingId) ?? null,
    [meetings, selectedMeetingId]
  );

  const fetchAllTranscripts = async (meetingId: string): Promise<Transcript[]> => {
    const firstPage = await invoke<MeetingTranscriptsResponse>('api_get_meeting_transcripts', {
      meetingId,
      limit: 1,
      offset: 0,
    });

    if (firstPage.total_count === 0) {
      return [];
    }

    const all = await invoke<MeetingTranscriptsResponse>('api_get_meeting_transcripts', {
      meetingId,
      limit: firstPage.total_count,
      offset: 0,
    });

    return all.transcripts;
  };

  const getSummaryLanguage = async (meetingId: string): Promise<string | null> => {
    try {
      const result = await invoke<{ language: string | null }>('api_get_meeting_summary_language', { meetingId });
      return result.language;
    } catch {
      return null;
    }
  };

  const buildTranscriptText = (transcripts: Transcript[]): string => {
    return transcripts
      .map((entry) => {
        if (entry.audio_start_time === undefined) {
          return `${entry.timestamp} ${entry.text}`;
        }

        const totalSeconds = Math.floor(entry.audio_start_time);
        const minutes = Math.floor(totalSeconds / 60)
          .toString()
          .padStart(2, '0');
        const seconds = (totalSeconds % 60).toString().padStart(2, '0');

        return `[${minutes}:${seconds}] ${entry.text}`;
      })
      .join('\n');
  };

  const waitForSummary = async (meetingId: string): Promise<string> => {
    const maxPolls = 200;

    for (let i = 0; i < maxPolls; i += 1) {
      const result = await invoke<SummaryStatusResponse>('api_get_summary', { meetingId });

      if (result.status === 'completed') {
        const generated = result.data?.markdown;
        if (!generated) throw new Error('Summary finished without markdown content.');
        return generated;
      }

      if (result.status === 'error' || result.status === 'failed') {
        throw new Error(result.error ?? 'Skill generation failed.');
      }

      await new Promise((resolve) => setTimeout(resolve, 1500));
    }

    throw new Error('Skill generation timed out after 5 minutes.');
  };

  const handleGenerateSkill = async () => {
    if (!selectedMeetingId) {
      toast.error('Please select a meeting first.');
      return;
    }

    setIsGenerating(true);
    try {
      const transcripts = await fetchAllTranscripts(selectedMeetingId);
      if (!transcripts.length) {
        toast.error('No transcripts found for this meeting.');
        return;
      }

      const transcriptText = buildTranscriptText(transcripts);
      const summaryLanguage = await getSummaryLanguage(selectedMeetingId);

      const start = await invoke<ProcessTranscriptResponse>('api_process_transcript', {
        text: transcriptText,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        meetingId: selectedMeetingId,
        chunkSize: 40000,
        overlap: 1000,
        customPrompt: '',
        templateId: 'skill_generator',
        summaryLanguage,
      });

      const generatedMarkdown = await waitForSummary(start.process_id);
      setMarkdown(generatedMarkdown);

      const inferred = sanitizeDisplayName(inferSkillName(generatedMarkdown));
      if (inferred) {
        setSkillName(inferred);
      }

      toast.success('Skill draft generated.');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error('Failed to generate skill.', { description: message });
    } finally {
      setIsGenerating(false);
    }
  };

  const callExport = async (overwrite: boolean) => {
    return invoke<string>('export_skill', {
      skillName,
      markdown,
      overwrite,
    });
  };

  const handleExport = async () => {
    if (!skillName.trim()) {
      toast.error('Please enter a skill name.');
      return;
    }

    if (!markdown.trim()) {
      toast.error('Generate or edit skill markdown before exporting.');
      return;
    }

    setIsExporting(true);
    try {
      let outputPath: string;

      try {
        outputPath = await callExport(false);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (!message.includes('SKILL_ALREADY_EXISTS:')) throw error;

        const confirmOverwrite = window.confirm('A skill with this name already exists. Overwrite it?');
        if (!confirmOverwrite) return;

        outputPath = await callExport(true);
      }

      toast.success('Skill exported successfully.', {
        description: outputPath,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error('Failed to export skill.', { description: message });
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <div className="h-screen overflow-y-auto bg-gray-50 p-8">
      <div className="mx-auto max-w-6xl space-y-6">
        <div className="flex items-center justify-between">
          <button
            onClick={() => router.back()}
            className="inline-flex items-center gap-2 text-gray-600 hover:text-gray-900"
          >
            <ArrowLeft className="h-4 w-4" />
            Back
          </button>
        </div>

        <div>
          <h1 className="text-3xl font-bold text-gray-900">Skills</h1>
          <p className="mt-1 text-sm text-gray-600">Generate, edit, and export reusable SKILL.md documents.</p>
        </div>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
          <div className="rounded-lg border bg-white p-4 shadow-sm lg:col-span-1 space-y-4">
            <div>
              <label className="mb-1 block text-sm font-medium text-gray-700">Source meeting</label>
              <select
                value={selectedMeetingId}
                onChange={(e) => setSelectedMeetingId(e.target.value)}
                className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm"
              >
                <option value="">Select a meeting</option>
                {meetings.map((meeting) => (
                  <option key={meeting.id} value={meeting.id}>
                    {meeting.title}
                  </option>
                ))}
              </select>
            </div>

            <button
              onClick={handleGenerateSkill}
              disabled={isGenerating || !selectedMeetingId}
              className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-blue-300"
            >
              <Sparkles className="h-4 w-4" />
              {isGenerating ? 'Generating...' : 'Generate Skill'}
            </button>

            {selectedMeeting && (
              <p className="text-xs text-gray-500">
                Using transcript from: <span className="font-medium">{selectedMeeting.title}</span>
              </p>
            )}

            <div>
              <label className="mb-1 block text-sm font-medium text-gray-700">Skill name</label>
              <input
                value={skillName}
                onChange={(e) => setSkillName(sanitizeDisplayName(e.target.value))}
                placeholder="deploy-staging-server"
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
              />
            </div>

            <button
              onClick={handleExport}
              disabled={isExporting || !markdown.trim() || !skillName.trim()}
              className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700 disabled:cursor-not-allowed disabled:bg-emerald-300"
            >
              <Download className="h-4 w-4" />
              {isExporting ? 'Exporting...' : 'Export to ~/.hermes/skills'}
            </button>
          </div>

          <div className="rounded-lg border bg-white p-4 shadow-sm lg:col-span-2 space-y-4">
            <h2 className="text-lg font-semibold text-gray-900">SKILL.md Editor & Preview</h2>
            <textarea
              value={markdown}
              onChange={(e) => setMarkdown(e.target.value)}
              className="min-h-[420px] w-full rounded-md border border-gray-300 p-3 font-mono text-sm"
              placeholder="Generated SKILL.md content will appear here..."
            />
          </div>
        </div>
      </div>
    </div>
  );
}
