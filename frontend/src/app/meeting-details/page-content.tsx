"use client";
import { useState, useEffect, useCallback, useRef } from 'react';
import { Transcript, Summary, SummaryResponse } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { TranscriptView } from '@/components/TranscriptView';
import { AISummary } from '@/components/AISummary';
import { BlockNoteSummaryView, BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { CurrentMeeting, useSidebar } from '@/components/Sidebar/SidebarProvider';
import { BlockNoteBlock } from '@/types';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '@/components/SettingTabs';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTrigger,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"
import { MessageToast } from '@/components/MessageToast';
import Analytics from '@/lib/analytics';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Copy, Sparkles, Settings, Save, Loader2 } from 'lucide-react';
import { toast } from 'sonner';


type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

export default function PageContent({ meeting, summaryData, onMeetingUpdated }: { meeting: any, summaryData: Summary, onMeetingUpdated?: () => Promise<void> }) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  const [transcripts, setTranscripts] = useState<Transcript[]>(meeting.transcripts);


  const [showSummary, setShowSummary] = useState(false);
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [meetingTitle, setMeetingTitle] = useState(meeting.title || '+ New Call');
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [aiSummary, setAiSummary] = useState<Summary | null>(summaryData);
  const [summaryResponse, setSummaryResponse] = useState<SummaryResponse | null>(null);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: 'llama3.2:latest',
    whisperModel: 'large-v3'
  });
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'localWhisper',
    model: 'large-v3',
  });
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [originalTranscript, setOriginalTranscript] = useState<string>('');
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const [isRecording, setIsRecording] = useState(false);
  const [error, setError] = useState<string>('');
  const [meetings, setLocalMeetings] = useState<CurrentMeeting[]>([]);
  const [settingsSaveSuccess, setSettingsSaveSuccess] = useState<boolean | null>(null);
  const { setCurrentMeeting, setMeetings, meetings: sidebarMeetings, serverAddress, startSummaryPolling } = useSidebar();

  // Ref for BlockNoteSummaryView to access its save method
  const blockNoteSummaryRef = useRef<BlockNoteSummaryViewRef>(null);
  const [isSummaryDirty, setIsSummaryDirty] = useState(false);

  // Keep local meetings state in sync with sidebar meetings
  useEffect(() => {
    setLocalMeetings(sidebarMeetings);
  }, [sidebarMeetings]);

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  // Combined effect to fetch both model and transcript configs
  useEffect(() => {
    // Set default configurations
    setModelConfig({
      provider: 'ollama',
      model: 'llama3.2:latest',
      whisperModel: 'large-v3'
    });
    const fetchModelConfig = async () => {
      try {
        const data = await invokeTauri('api_get_model_config', {}) as any;
        if (data && data.provider !== null) {
          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
    };

    fetchModelConfig();
  }, [serverAddress]);

  useEffect(() => {
    console.log('Model config:', modelConfig);
  }, [modelConfig]);

  useEffect(() => {

    setTranscriptModelConfig({
      provider: 'localWhisper',
      model: 'large-v3',
    });

    const fetchConfigurations = async () => {
      // Only make API call if serverAddress is loaded
      if (!serverAddress) {
        console.log('Waiting for server address to load before fetching configurations');
        return;
      }

      try {
        const data = await invokeTauri('api_get_transcript_config', {}) as any;
        if (data && data.provider !== null) {
          setTranscriptModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch configurations:', error);
      }
    };

    fetchConfigurations();
  }, [serverAddress]);

  // // Reset settings save success after showing toast
  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000); // Same duration as toast

  //     return () => clearTimeout(timer);
  //   }
  // }, [settingsSaveSuccess]);

  const generateAISummary = useCallback(async (customPrompt: string = '') => {
    setSummaryStatus('processing');
    setSummaryError(null);

    try {
      const fullTranscript = transcripts?.map(t => t.text).join('\n');
      if (!fullTranscript.trim()) {
        throw new Error('No transcript text available. Please add some text first.');
      }

      setOriginalTranscript(fullTranscript);

      console.log('Generating summary for transcript length:', fullTranscript.length);

      // Track summary generation started
      await Analytics.trackSummaryGenerationStarted(
        modelConfig.provider,
        modelConfig.model,
        fullTranscript.length
      );

      // Track custom prompt usage if present
      if (customPrompt.trim().length > 0) {
        await Analytics.trackCustomPromptUsed(customPrompt.trim().length);
      }

      // Process transcript and get process_id
      console.log('Processing transcript...');
      const result = await invokeTauri('api_process_transcript', {
        text: fullTranscript,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        meetingId: meeting.id,
        chunkSize: 40000,
        overlap: 1000,
        customPrompt: customPrompt,
      }) as any;

      const process_id = result.process_id;
      console.log('Process ID:', process_id);

      // Start global polling via context
      startSummaryPolling(meeting.id, process_id, async (pollingResult) => {
        console.log('Summary status:', pollingResult);
        console.log('Error from backend:', pollingResult.error);

        if (pollingResult.status === 'error' || pollingResult.status === 'failed') {
          console.error('Backend returned error:', pollingResult.error);
          const errorMessage = pollingResult.error || 'Summary generation failed';
          setSummaryError(errorMessage);
          setSummaryStatus('error');

          // Show error toast to user
          toast.error('Failed to generate summary', {
            description: errorMessage.includes('Connection refused')
              ? 'Could not connect to LLM service. Please ensure Ollama or your configured LLM provider is running.'
              : errorMessage,
          });

          // Track summary generation error
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            false,
            undefined,
            errorMessage
          );
          return;
        }

        if (pollingResult.status === 'completed' && pollingResult.data) {
          console.log('✅ Summary generation completed:', pollingResult.data);

          // Check if backend returned markdown format (new flow)
          if (pollingResult.data.markdown) {
            console.log('📝 Received markdown format from backend');

            // Update meeting title if available (check both data.MeetingName and top-level meetingName)
            const meetingName = pollingResult.data.MeetingName || pollingResult.meetingName;
            if (meetingName) {
              console.log('📝 Updating meeting title to:', meetingName);
              setMeetingTitle(meetingName);
              const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
                m.id === meeting.id ? { id: m.id, title: meetingName } : m
              );
              setMeetings(updatedMeetings);
              setCurrentMeeting({ id: meeting.id, title: meetingName });
            }

            // Set markdown data - BlockNoteSummaryView will parse and render
            setAiSummary({ markdown: pollingResult.data.markdown } as any);
            setSummaryStatus('completed');

            // Refetch meeting details from backend to sync updated title
            if (meetingName && onMeetingUpdated) {
              await onMeetingUpdated();
            }

            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              true
            );
            return;
          }

          // Legacy format handling (for backwards compatibility)
          const summarySections = Object.entries(pollingResult.data).filter(([key]) => key !== 'MeetingName');
          const allEmpty = summarySections.every(([, section]) => !(section as any).blocks || (section as any).blocks.length === 0);

          if (allEmpty) {
            console.error('Summary completed but all sections empty');
            setSummaryError('Summary generation completed but returned empty content.');
            setSummaryStatus('error');

            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              false,
              undefined,
              'Empty summary generated'
            );
            return;
          }

          // Remove MeetingName from data before formatting
          const { MeetingName, ...summaryData } = pollingResult.data;

          // Update meeting title if available (check both data.MeetingName and top-level meetingName)
          const meetingName = MeetingName || pollingResult.meetingName;
          if (meetingName) {
            console.log('📝 Updating meeting title to:', meetingName);
            setMeetingTitle(meetingName);
            const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
              m.id === meeting.id ? { id: m.id, title: meetingName } : m
            );
            setMeetings(updatedMeetings);
            setCurrentMeeting({ id: meeting.id, title: meetingName });
          }

          // Format legacy summary data
          const formattedSummary: Summary = {};
          const sectionKeys = pollingResult.data._section_order || Object.keys(summaryData);

          for (const key of sectionKeys) {
            try {
              const section = summaryData[key];
              if (section && typeof section === 'object' && 'title' in section && 'blocks' in section) {
                const typedSection = section as { title?: string; blocks?: any[] };

                if (Array.isArray(typedSection.blocks)) {
                  formattedSummary[key] = {
                    title: typedSection.title || key,
                    blocks: typedSection.blocks.map((block: any) => ({
                      ...block,
                      color: 'default',
                      content: block?.content?.trim() || ''
                    }))
                  };
                } else {
                  formattedSummary[key] = {
                    title: typedSection.title || key,
                    blocks: []
                  };
                }
              }
            } catch (error) {
              console.warn(`Error processing section ${key}:`, error);
            }
          }

          setAiSummary(formattedSummary);
          setSummaryStatus('completed');

          // Track successful summary generation
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            true
          );

          // Refetch meeting details from backend to sync updated title
          if (meetingName && onMeetingUpdated) {
            await onMeetingUpdated();
          }
        }
      });
    } catch (error) {
      console.error('Failed to generate summary:', error);
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      const fullError = `Failed to generate summary: ${errorMessage}`;
      setSummaryError(fullError);
      setSummaryStatus('error');

      // Show error toast to user
      toast.error('Failed to generate summary', {
        description: errorMessage,
      });

      // Track summary generation error
      await Analytics.trackSummaryGenerationCompleted(
        modelConfig.provider,
        modelConfig.model,
        false,
        undefined,
        errorMessage
      );
    }
  }, [transcripts, modelConfig, meeting.id, startSummaryPolling, sidebarMeetings, setMeetings, setCurrentMeeting]);


  const handleSaveSummary = async (summary: Summary | { markdown?: string; summary_json?: BlockNoteBlock[] }) => {
    console.log('📄 PAGE CONTENT: handleSaveSummary called with:', {
      hasMarkdown: 'markdown' in summary,
      hasSummaryJson: 'summary_json' in summary,
      summaryKeys: Object.keys(summary)
    });

    try {
      let formattedSummary: any;

      // Check if it's the new BlockNote format
      if ('markdown' in summary || 'summary_json' in summary) {
        console.log('📄 PAGE CONTENT: Saving new format (markdown/blocknote)');
        // New format: save as-is with markdown and/or summary_json keys
        formattedSummary = summary;
      } else {
        console.log('📄 PAGE CONTENT: Saving legacy format');
        // Legacy format: structure for backward compatibility
        formattedSummary = {
          MeetingName: meetingTitle,
          MeetingNotes: {
            sections: Object.entries(summary).map(([, section]) => ({
              title: section.title,
              blocks: section.blocks
            }))
          }
        };
      }

      const payload = {
        meetingId: meeting.id,
        summary: formattedSummary
      };
      console.log('📄 PAGE CONTENT: Saving summary payload:', payload);

      await invokeTauri('api_save_meeting_summary', {
        meetingId: payload.meetingId,
        summary: payload.summary,
      });

      console.log('✅ PAGE CONTENT: Save meeting summary success');
    } catch (error) {
      console.error('❌ PAGE CONTENT: Failed to save meeting summary:', error);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save meeting summary: Unknown error');
      }
    }
  };

  const handleSummaryChange = (newSummary: Summary) => {
    setAiSummary(newSummary);
    // Track summary editing
    Analytics.trackFeatureUsed('summary_edited');
  };

  const handleTitleChange = (newTitle: string) => {
    setMeetingTitle(newTitle);
  };

  const getSummaryStatusMessage = (status: SummaryStatus) => {
    switch (status) {
      case 'processing':
        return 'Processing transcript...';
      case 'summarizing':
        return 'Generating summary...';
      case 'regenerating':
        return 'Regenerating summary...';
      case 'completed':
        return 'Summary completed';
      case 'error':
        return 'Error generating summary';
      default:
        return '';
    }
  };

  const handleRegenerateSummary = useCallback(async () => {
    if (!originalTranscript.trim()) {
      console.error('No original transcript available for regeneration');
      return;
    }

    setSummaryStatus('regenerating');
    setSummaryError(null);

    try {
      console.log('Regenerating summary with original transcript...');

      // Track summary regeneration started
      await Analytics.trackSummaryGenerationStarted(
        modelConfig.provider,
        modelConfig.model,
        originalTranscript.length
      );

      // Process transcript and get process_id
      console.log('Processing transcript...');
      const result = await invokeTauri('api_process_transcript', {
        text: originalTranscript,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        meetingId: meeting.id,
        chunkSize: 40000,
        overlap: 1000,
      }) as any;

      const process_id = result.process_id;
      console.log('Process ID:', process_id);

      // Start global polling via context
      startSummaryPolling(meeting.id, process_id, async (pollingResult) => {
        console.log('Summary status:', pollingResult);
        console.log('Error from backend:', pollingResult.error);

        if (pollingResult.status === 'error' || pollingResult.status === 'failed') {
          console.error('Backend returned error:', pollingResult.error);
          const errorMessage = pollingResult.error || 'Summary regeneration failed';
          setSummaryError(errorMessage);
          setSummaryStatus('error');

          // Show error toast to user
          toast.error('Failed to regenerate summary', {
            description: errorMessage.includes('Connection refused')
              ? 'Could not connect to LLM service. Please ensure Ollama or your configured LLM provider is running.'
              : errorMessage,
          });

          // Track summary regeneration error
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            false,
            undefined,
            errorMessage
          );
          return;
        }

        if (pollingResult.status === 'completed' && pollingResult.data) {
          // Remove MeetingName from data before formatting
          const { MeetingName, ...summaryData } = pollingResult.data;

          // Update meeting title if available (check both data.MeetingName and top-level meetingName)
          const meetingName = MeetingName || pollingResult.meetingName;
          if (meetingName) {
            console.log('📝 Updating meeting title to:', meetingName);
            setMeetingTitle(meetingName);
            // Update meetings with new title
            const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
              m.id === meeting.id ? { id: m.id, title: meetingName } : m
            );
            setMeetings(updatedMeetings);
            setCurrentMeeting({ id: meeting.id, title: meetingName });
          }

          // Format the summary data with consistent styling - PRESERVE ORDER
          const formattedSummary: Summary = {};

          // Use section order if available to maintain exact order and handle duplicates
          const sectionKeys = pollingResult.data._section_order || Object.keys(summaryData);

          for (const key of sectionKeys) {
            try {
              const section = summaryData[key];
              // Comprehensive null checks to prevent errors
              if (section &&
                typeof section === 'object' &&
                'title' in section &&
                'blocks' in section) {

                const typedSection = section as { title?: string; blocks?: any[] };

                // Ensure blocks is an array before mapping
                if (Array.isArray(typedSection.blocks)) {
                  formattedSummary[key] = {
                    title: typedSection.title || key,
                    blocks: typedSection.blocks.map((block: any) => ({
                      ...block,
                      // type: 'bullet',
                      color: 'default',
                      content: block?.content?.trim() || '' // Handle null content
                    }))
                  };
                } else {
                  // Handle case where blocks is not an array
                  console.warn(`Section ${key} has invalid blocks:`, typedSection.blocks);
                  formattedSummary[key] = {
                    title: typedSection.title || key,
                    blocks: []
                  };
                }
              } else {
                console.warn(`Skipping invalid section ${key}:`, section);
              }
            } catch (error) {
              console.warn(`Error processing section ${key}:`, error);
              // Continue processing other sections
            }
          }

          setAiSummary(formattedSummary);
          setSummaryStatus('completed');

          // Track successful summary regeneration
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            true
          );

          // Refetch meeting details from backend to sync updated title
          if (meetingName && onMeetingUpdated) {
            await onMeetingUpdated();
          }
        }
      });
    } catch (error) {
      console.error('Failed to regenerate summary:', error);
      const errorMessage = error instanceof Error ? error.message : 'An unexpected error occurred';
      setSummaryError(errorMessage);
      setSummaryStatus('error');
      setAiSummary(null);

      // Show error toast to user
      toast.error('Failed to regenerate summary', {
        description: errorMessage,
      });

      // Track summary regeneration error
      await Analytics.trackSummaryGenerationCompleted(
        modelConfig.provider,
        modelConfig.model,
        false,
        undefined,
        errorMessage
      );
    }
  }, [originalTranscript, modelConfig, meeting.id, startSummaryPolling, sidebarMeetings, setMeetings, setCurrentMeeting]);

  const handleCopyTranscript = useCallback(() => {
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

    const header = `# Transcript of the Meeting: ${meeting.id} - ${meetingTitle??meeting.title}\n\n`;
    const date = `## Date: ${new Date(meeting.created_at).toLocaleDateString()}\n\n`;
    const fullTranscript = transcripts
      .map(t => `${formatTime(t.audio_start_time, t.timestamp)} ${t.text}`)
      .join('\n');
    navigator.clipboard.writeText(header + date + fullTranscript);

    toast.success("Transcript copied to clipboard")
  }, [transcripts, meeting, meetingTitle]);

  const handleCopySummary = useCallback(async () => {
    try {
      let summaryMarkdown = '';

      console.log('🔍 Copy Summary - Starting...');
      console.log('🔍 aiSummary:', aiSummary);
      console.log('🔍 aiSummary type:', typeof aiSummary);
      console.log('🔍 aiSummary keys:', aiSummary ? Object.keys(aiSummary) : 'null');
      console.log('🔍 blockNoteSummaryRef.current:', blockNoteSummaryRef.current);

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
          .map(([key, section]) => {
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
        console.error('❌ Debug - aiSummary:', JSON.stringify(aiSummary, null, 2));
        setSaveSuccess(false);
        setTimeout(() => setSaveSuccess(null), 2000);
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
      toast.success("Summary copied to clipboard")

      // Track analytics
      Analytics.trackFeatureUsed('summary_copied');
    } catch (error) {
      console.error('❌ Failed to copy summary:', error);
      toast.error("Failed to copy summary")
    }
  }, [aiSummary, meetingTitle, meeting, blockNoteSummaryRef]);

  const handleGenerateSummary = useCallback(async (customPrompt: string = '') => {
    if (!transcripts.length) {
      let error_msg = 'No transcripts available for summary';
      console.log(error_msg);
      toast.error(error_msg)
      return;
    }

    try {
      await generateAISummary(customPrompt);
    } catch (error) {
      let error_msg = "Failed to generate summary"
      console.error(error_msg, error);
      if (error instanceof Error) {
        setSummaryError(error.message);
        toast.error(error_msg, { description: error.message })
      } else {
        setSummaryError('Failed to generate summary: Unknown error');
      }
    }
  }, [transcripts, generateAISummary]);

  const handleSaveMeetingTitle = async () => {
    try {
      const payload = {
        meetingId: meeting.id,
        title: meetingTitle
      };
      console.log('Saving meeting title with payload:', payload);

      await invokeTauri('api_save_meeting_title', {
        meetingId: meeting.id,
        title: meetingTitle,
      });

      console.log('Save meeting title success');


      // Update meetings with new title
      const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
        m.id === meeting.id ? { id: m.id, title: meetingTitle } : m
      );
      setMeetings(updatedMeetings);
      setCurrentMeeting({ id: meeting.id, title: meetingTitle });
      return true;
    } catch (error) {
      console.error('Failed to save meeting title:', error);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save meeting title: Unknown error');
      }
      return false;
    }
  };

  // Function to save all changes (title and summary)
  const [isSaving, setIsSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState<boolean | null>(null);

  const saveAllChanges = async () => {
    setIsSaving(true);
    try {
      // Save meeting title
      await handleSaveMeetingTitle();

      // Save BlockNote editor changes if dirty
      if (blockNoteSummaryRef.current?.isDirty) {
        console.log('💾 Saving BlockNote editor changes...');
        await blockNoteSummaryRef.current.saveSummary();
      } else if (aiSummary) {
        // Fallback for legacy summary format
        await handleSaveSummary(aiSummary);
      }

      toast.success("Changes saved successfully")
    } catch (error) {
      console.error('Failed to save changes:', error);
      toast.error("Failed to save changes", { description: String(error) })
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveModelConfig = async (updatedConfig?: ModelConfig) => {
    try {
      const configToSave = updatedConfig || modelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        whisperModel: configToSave.whisperModel,
        apiKey: configToSave.apiKey ?? null
      };
      console.log('Saving model config with payload:', payload);

      // Track model configuration change
      if (updatedConfig && (
        updatedConfig.provider !== modelConfig.provider ||
        updatedConfig.model !== modelConfig.model
      )) {
        await Analytics.trackModelChanged(
          modelConfig.provider,
          modelConfig.model,
          updatedConfig.provider,
          updatedConfig.model
        );
      }

      await invokeTauri('api_save_model_config', {
        provider: payload.provider,
        model: payload.model,
        whisperModel: payload.whisperModel,
        apiKey: payload.apiKey,
      });

      console.log('Save model config success');
      setSettingsSaveSuccess(true);
      setModelConfig(payload);

      await Analytics.trackSettingsChanged('model_config', `${payload.provider}_${payload.model}`);


    } catch (error) {
      console.error('Failed to save model config:', error);
      setSettingsSaveSuccess(false);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save model config: Unknown error');
      }
    }
  };

  const handleSaveTranscriptConfig = async (updatedConfig?: TranscriptModelProps) => {
    try {
      const configToSave = updatedConfig || transcriptModelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        apiKey: configToSave.apiKey ?? null
      };
      console.log('Saving transcript config with payload:', payload);


      await invokeTauri('api_save_transcript_config', {
        provider: payload.provider,
        model: payload.model,
        api_key: payload.apiKey,
      });


      console.log('Save transcript config success');
      setSettingsSaveSuccess(true);
      const transcriptConfigToSave = updatedConfig || transcriptModelConfig;
      await Analytics.trackSettingsChanged('transcript_config', `${transcriptConfigToSave.provider}_${transcriptConfigToSave.model}`);
    } catch (error) {
      console.error('Failed to save transcript config:', error);
      setSettingsSaveSuccess(false);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save transcript config: Unknown error');
      }
    }
  };
  const isSummaryLoading = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

  return (
    <div className="flex flex-col h-screen bg-gray-50">
      <div className="flex flex-1 overflow-hidden">
        {/* Left side - Transcript */}
        <div className="w-1/3 min-w-[300px] border-r border-gray-200 bg-white flex flex-col relative">
          {/* Title area */}
          <div className="p-4 border-b border-gray-200">
            <div className="flex flex-col space-y-3">

              <div className="flex items-center space-x-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    Analytics.trackButtonClick('copy_transcript', 'meeting_details');
                    handleCopyTranscript();
                  }}
                  disabled={transcripts?.length === 0}
                  title={transcripts?.length === 0 ? 'No transcript available' : 'Copy Transcript'}
                >
                  <Copy />
                  Copy
                </Button>
                {transcripts?.length > 0 && (
                  <>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        Analytics.trackButtonClick('generate_summary', 'meeting_details');
                        handleGenerateSummary(customPrompt);
                      }}
                      disabled={summaryStatus === 'processing'}
                      title={
                        summaryStatus === 'processing'
                          ? 'Generating summary...'
                          : 'Generate AI Summary'
                      }
                    >
                      {summaryStatus === 'processing' ? (
                        <>
                          <Loader2 className="animate-spin" />
                          Processing...
                        </>
                      ) : (
                        <>
                          <Sparkles />
                          Generate Note
                        </>
                      )}
                    </Button>
                    <Dialog>
                      <DialogTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          title="Settings"
                        >
                          <Settings />
                        </Button>
                      </DialogTrigger>
                      <DialogContent
                        aria-describedby={undefined}
                      >
                        <VisuallyHidden>
                          <DialogTitle>Model Settings</DialogTitle>
                        </VisuallyHidden>
                        <SettingTabs
                          modelConfig={modelConfig}
                          setModelConfig={setModelConfig}
                          onSave={handleSaveModelConfig}
                          transcriptModelConfig={transcriptModelConfig}
                          setTranscriptModelConfig={setTranscriptModelConfig}
                          onSaveTranscript={handleSaveTranscriptConfig}
                          setSaveSuccess={setSettingsSaveSuccess}
                        />
                        {settingsSaveSuccess !== null && (
                          <DialogFooter>
                            <MessageToast
                              message={settingsSaveSuccess ? 'Settings saved successfully' : 'Failed to save settings'}
                              type={settingsSaveSuccess ? 'success' : 'error'}
                              show={settingsSaveSuccess !== null}
                              setShow={() => setSettingsSaveSuccess(null)}
                            />
                          </DialogFooter>
                        )}
                      </DialogContent>


                    </Dialog>

                  </>
                )}
              </div>
            </div>
          </div>

          {/* Transcript content */}
          <div className="flex-1 overflow-y-auto pb-4">
            <TranscriptView transcripts={transcripts} />
          </div>

          {/* Custom prompt input at bottom of transcript section */}
          {!isRecording && transcripts.length > 0 && (
            <div className="p-1 border-t border-gray-200">
              <textarea
                placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
                className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
                value={customPrompt}
                onChange={(e) => setCustomPrompt(e.target.value)}
                disabled={summaryStatus === 'processing'}
              />
            </div>
          )}
        </div>

        {/* Right side - AI Summary */}
        <div className="flex-1 overflow-y-auto bg-white">
          <div className="p-4 border-b border-gray-200">
            <div className="flex items-center justify-between">
              <div className="flex-1">
                <EditableTitle
                  title={meetingTitle}
                  isEditing={isEditingTitle}
                  onStartEditing={() => setIsEditingTitle(true)}
                  onFinishEditing={() => setIsEditingTitle(false)}
                  onChange={handleTitleChange}
                />
              </div>
              <div className="flex flex-col items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    Analytics.trackButtonClick('save_changes', 'meeting_details');
                    saveAllChanges();
                  }}
                  disabled={isSaving}
                >
                  {isSaving ? (
                    <>
                      <Loader2 className="animate-spin" />
                      Saving...
                    </>
                  ) : (
                    <>
                      <Save />
                      Save Changes
                    </>
                  )}
                </Button>
                {

                  <Button
                    variant='outline'
                    size={"sm"}
                    title='Copy Summary'
                    onClick={() => {
                      Analytics.trackButtonClick('copy_summary', 'meeting_details');
                      handleCopySummary();
                    }}
                    disabled={!aiSummary || summaryStatus === 'processing'}
                    className='cursor-pointer'
                  >
                    <Copy />
                    Copy summary
                  </Button>
                }
              </div>
            </div>
          </div>
          {isSummaryLoading ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mb-4"></div>
                <p className="text-gray-600">Generating AI Summary...</p>
              </div>
            </div>
          ) : transcripts?.length > 0 && (
            <div className="max-w-4xl mx-auto p-6">
              {summaryResponse && (
                <div className="fixed bottom-0 left-0 right-0 bg-white shadow-lg p-4 max-h-1/3 overflow-y-auto">
                  <h3 className="text-lg font-semibold mb-2">Meeting Summary</h3>
                  <div className="grid grid-cols-2 gap-4">
                    <div className="bg-white p-4 rounded-lg shadow-sm">
                      <h4 className="font-medium mb-1">Key Points</h4>
                      <ul className="list-disc pl-4">
                        {summaryResponse.summary.key_points.blocks.map((block, i) => (
                          <li key={i} className="text-sm">{block.content}</li>
                        ))}
                      </ul>
                    </div>
                    <div className="bg-white p-4 rounded-lg shadow-sm mt-4">
                      <h4 className="font-medium mb-1">Action Items</h4>
                      <ul className="list-disc pl-4">
                        {summaryResponse.summary.action_items.blocks.map((block, i) => (
                          <li key={i} className="text-sm">{block.content}</li>
                        ))}
                      </ul>
                    </div>
                    <div className="bg-white p-4 rounded-lg shadow-sm mt-4">
                      <h4 className="font-medium mb-1">Decisions</h4>
                      <ul className="list-disc pl-4">
                        {summaryResponse.summary.decisions.blocks.map((block, i) => (
                          <li key={i} className="text-sm">{block.content}</li>
                        ))}
                      </ul>
                    </div>
                    <div className="bg-white p-4 rounded-lg shadow-sm mt-4">
                      <h4 className="font-medium mb-1">Main Topics</h4>
                      <ul className="list-disc pl-4">
                        {summaryResponse.summary.main_topics.blocks.map((block, i) => (
                          <li key={i} className="text-sm">{block.content}</li>
                        ))}
                      </ul>
                    </div>
                  </div>
                  {summaryResponse.raw_summary ? (
                    <div className="mt-4">
                      <h4 className="font-medium mb-1">Full Summary</h4>
                      <p className="text-sm whitespace-pre-wrap">{summaryResponse.raw_summary}</p>
                    </div>
                  ) : null}
                </div>
              )}
              <div className="flex-1 overflow-y-auto">
                <BlockNoteSummaryView
                  ref={blockNoteSummaryRef}
                  summaryData={aiSummary}
                  onSave={handleSaveSummary}
                  onSummaryChange={handleSummaryChange}
                  onDirtyChange={setIsSummaryDirty}
                  status={summaryStatus}
                  error={summaryError}
                  onRegenerateSummary={() => {
                    Analytics.trackButtonClick('regenerate_summary', 'meeting_details');
                    handleRegenerateSummary();
                  }}
                  meeting={{
                    id: meeting.id,
                    title: meetingTitle,
                    created_at: meeting.created_at
                  }}
                />
              </div>
              {summaryStatus !== 'idle' && (
                <div className={`mt-4 p-4 rounded-lg ${summaryStatus === 'error' ? 'bg-red-100 text-red-700' :
                  summaryStatus === 'completed' ? 'bg-green-100 text-green-700' :
                    'bg-blue-100 text-blue-700'
                  }`}>
                  <p className="text-sm font-medium">{getSummaryStatusMessage(summaryStatus)}</p>
                </div>
              )}
            </div>
          )}
        </div>

      </div>
    </div>
  );
}



