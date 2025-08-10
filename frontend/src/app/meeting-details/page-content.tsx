"use client";
import { useState, useEffect, useCallback } from 'react';
import { debounce, invoke } from 'lodash';
import { Transcript, Summary, SummaryResponse } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { TranscriptView } from '@/components/TranscriptView';
import { AISummary } from '@/components/AISummary';
import { CurrentMeeting, useSidebar } from '@/components/Sidebar/SidebarProvider';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '@/components/SettingTabs';
import {TranscriptModelProps } from '@/components/TranscriptSettings';
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


type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

export default function PageContent({ meeting, summaryData }: { meeting: any, summaryData: Summary }) {
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
  const { setCurrentMeeting, setMeetings, meetings: sidebarMeetings , serverAddress} = useSidebar();
  
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

      // Poll for summary status
      const pollInterval = setInterval(async () => {
        try {
          const result = await invokeTauri('api_get_summary', {
            meetingId: process_id,
          }) as any;
          console.log('Summary status:', result);

          if (result.status === 'error') {
            setSummaryError(result.error || 'Unknown error');
            setSummaryStatus('error');
            clearInterval(pollInterval);
            
            // Track summary generation error
            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              false,
              undefined,
              result.error || 'Unknown error'
            );
            return;
          }

          if (result.status === 'completed' && result.data) {
            // Defensive: check if all sections are empty
            const summarySections = Object.entries(result.data).filter(([key]) => key !== 'MeetingName');
            const allEmpty = summarySections.every(([, section]) => !(section as any).blocks || (section as any).blocks.length === 0);
            if (allEmpty) {
              setSummaryError('Summary generation failed. Please check your model/API key settings.');
              setSummaryStatus('error');
              clearInterval(pollInterval);
              
              // Track summary generation failure
              await Analytics.trackSummaryGenerationCompleted(
                modelConfig.provider,
                modelConfig.model,
                false,
                undefined,
                'Empty summary generated'
              );
              return;
            }
            clearInterval(pollInterval);

            // Remove MeetingName from data before formatting
            const { MeetingName, ...summaryData } = result.data;

            // Update meeting title if available
            if (MeetingName) {
              setMeetingTitle(MeetingName);
              // Update meetings with new title
              const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) => 
                m.id === meeting.id ? { id: m.id, title: MeetingName } : m
              );
              setMeetings(updatedMeetings);
              setCurrentMeeting({ id: meeting.id, title: MeetingName });
            }
            
            // Format the summary data with consistent styling
            const formattedSummary = Object.entries(summaryData).reduce((acc: Summary, [key, section]: [string, any]) => {
              if (section && section.title) {
                acc[key] = {
                  title: section.title,
                  blocks: (section.blocks || []).map((block: any) => ({
                    ...block,
                    // type: 'bullet',
                    color: 'default',
                    content: block?.content?.trim() || '' // Remove trailing newlines and handle null content
                  }))
                };
              }
              return acc;
            }, {} as Summary);

            setAiSummary(formattedSummary);
            setSummaryStatus('completed');
            
            // Track successful summary generation
            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              true
            );
          }
        } catch (error) {
          console.error('Failed to get summary status:', error);
          if (error instanceof Error) {
            setSummaryError(`Failed to get summary status: ${error.message}`);
          } else {
            setSummaryError('Failed to get summary status: Unknown error');
          }
          setSummaryStatus('error');
          clearInterval(pollInterval);
          
          // Track summary generation error
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            false,
            undefined,
            error instanceof Error ? error.message : 'Unknown error'
          );

        }
      }, 5000); // Poll every 5 seconds

      // Cleanup interval on component unmount
      return () => clearInterval(pollInterval);
    } catch (error) {
      console.error('Failed to generate summary:', error);
      if (error instanceof Error) {
        setSummaryError(`Failed to generate summary: ${error.message}`);
      } else {
        setSummaryError('Failed to generate summary: Unknown error');
      }
      setSummaryStatus('error');
      
      // Track summary generation error
      await Analytics.trackSummaryGenerationCompleted(
        modelConfig.provider,
        modelConfig.model,
        false,
        undefined,
        error instanceof Error ? error.message : 'Unknown error'
      );
    }
  }, [transcripts, modelConfig, meeting.id]);

  const handleSummary = useCallback((summary: any) => {
    setAiSummary(summary);
  }, []);

  const handleSaveSummary = async (summary: Summary) => {
    try {
      // Format the summary in a structure that the backend expects
      const formattedSummary = {
        MeetingName: meetingTitle,
        MeetingNotes: {
          sections: Object.entries(summary).map(([key, section]) => ({
            title: section.title,
            blocks: section.blocks
          }))
        }
      };
      
      const payload = {
        meetingId: meeting.id,
        summary: formattedSummary
      };
      console.log('Saving meeting summary with payload:', payload);
      
      await invokeTauri('api_save_meeting_summary', {
        meetingId: payload.meetingId,
        summary: payload.summary,
      });

      console.log('Save meeting summary success');
    } catch (error) {
      console.error('Failed to save meeting summary:', error);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save meeting summary: Unknown error');
      }
    }
  };

  // Create a debounced version of the save function to avoid excessive API calls
  const debouncedSaveSummary = useCallback(
    debounce((summary: Summary) => {
      handleSaveSummary(summary);
    }, 2000),
    [meeting.id, handleSaveSummary]
  );

  const handleSummaryChange = (newSummary: Summary) => {
    setAiSummary(newSummary);
    debouncedSaveSummary(newSummary);
    
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

      // Poll for summary status
      const pollInterval = setInterval(async () => {
        try {
          const result = await invokeTauri('api_get_summary', {
            meetingId: process_id,
          }) as any;
          console.log('Summary status:', result);

          if (result.status === 'error') {
            setSummaryError(result.error || 'Unknown error');
            setSummaryStatus('error');
            clearInterval(pollInterval);
            return;
          }

          if (result.status === 'completed' && result.data) {
            clearInterval(pollInterval);
            
            // Remove MeetingName from data before formatting
            const { MeetingName, ...summaryData } = result.data;
            
            // Update meeting title if available
            if (MeetingName) {
              setMeetingTitle(MeetingName);
              // Update meetings with new title
              const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) => 
                m.id === meeting.id ? { id: m.id, title: MeetingName } : m
              );
              setMeetings(updatedMeetings);
              setCurrentMeeting({ id: meeting.id, title: MeetingName });
            }

            // Format the summary data with consistent styling
            const formattedSummary = Object.entries(summaryData).reduce((acc: Summary, [key, section]: [string, any]) => {
              if (section && section.title) {
                acc[key] = {
                  title: section.title,
                  blocks: (section.blocks || []).map((block: any) => ({
                    ...block,
                    // type: 'bullet',
                    color: 'default',
                    content: block?.content?.trim() || '' // Handle null content
                  }))
                };
              }
              return acc;
            }, {} as Summary);

            setAiSummary(formattedSummary);
            setSummaryStatus('completed');
            
            // Track successful summary regeneration
            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              true
            );
          } else if (result.status === 'error') {
            clearInterval(pollInterval);
            
            // Track summary regeneration error
            await Analytics.trackSummaryGenerationCompleted(
              modelConfig.provider,
              modelConfig.model,
              false,
              undefined,
              result.error || 'Failed to generate summary'
            );
            throw new Error(result.error || 'Failed to generate summary');
          }
        } catch (error) {
          clearInterval(pollInterval);
          console.error('Failed to get summary status:', error);
          if (error instanceof Error) {
            setSummaryError(error.message);
          } else {
            setSummaryError('An unexpected error occurred');
          }
          setSummaryStatus('error');
          setAiSummary(null);
          
          // Track summary regeneration error
          await Analytics.trackSummaryGenerationCompleted(
            modelConfig.provider,
            modelConfig.model,
            false,
            undefined,
            error instanceof Error ? error.message : 'An unexpected error occurred'
          );
        }
      }, 10000);

      return () => clearInterval(pollInterval);
    } catch (error) {
      console.error('Failed to regenerate summary:', error);
      if (error instanceof Error) {
        setSummaryError(error.message);
      } else {
        setSummaryError('An unexpected error occurred');
      }
      setSummaryStatus('error');
      setAiSummary(null);
      
      // Track summary regeneration error
      await Analytics.trackSummaryGenerationCompleted(
        modelConfig.provider,
        modelConfig.model,
        false,
        undefined,
        error instanceof Error ? error.message : 'An unexpected error occurred'
      );
    }
  }, [originalTranscript, modelConfig, meeting.id]);

  const handleCopyTranscript = useCallback(() => {
    const header = `# Transcript of the Meeting: ${meeting.id} - ${meetingTitle??meeting.title}\n\n`;
    const date = `## Date: ${new Date(meeting.created_at).toLocaleDateString()}\n\n`;
    const fullTranscript = transcripts
      .map(t => `${t.timestamp}: ${t.text}`)
      .join('\n');
    navigator.clipboard.writeText(header + date + fullTranscript);
  }, [transcripts, meeting, meetingTitle]);

  const handleGenerateSummary = useCallback(async (customPrompt: string = '') => {
    if (!transcripts.length) {
      console.log('No transcripts available for summary');
      return;
    }
    
    try {
      await generateAISummary(customPrompt);
    } catch (error) {
      console.error('Failed to generate summary:', error);
      if (error instanceof Error) {
        setSummaryError(error.message);
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
    setSaveSuccess(null);
    
    try {
      // Save meeting title
      const titleSaved = await handleSaveMeetingTitle();
      
      // Save summary if it exists
      let summarySaved = true;
      if (aiSummary) {
        await handleSaveSummary(aiSummary);
      }
      
      setSaveSuccess(titleSaved && summarySaved);
      
      // Show success message briefly
      setTimeout(() => {
        setSaveSuccess(null);
      }, 3000);
    } catch (error) {
      console.error('Failed to save changes:', error);
      setSaveSuccess(false);
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
                <button
                  onClick={() => {
                    Analytics.trackButtonClick('copy_transcript', 'meeting_details');
                    handleCopyTranscript();
                  }}
                  disabled={transcripts?.length === 0}
                  className={`px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm ${
                    transcripts?.length === 0
                      ? 'bg-gray-50 border-gray-200 text-gray-400 cursor-not-allowed'
                      : 'bg-blue-50 border-blue-200 text-blue-700 hover:bg-blue-100 hover:border-blue-300 active:bg-blue-200'
                  }`}
                  title={transcripts?.length === 0 ? 'No transcript available' : 'Copy Transcript'}
                >
                  <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" strokeWidth="2" stroke="currentColor" fill="none">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 3h-6a2.25 2.25 0 0 0-2.25 2.25v13.5A2.25 2.25 0 0 0 7.5 21h6a2.25 2.25 0 0 0 2.25-2.25V7.5l-3.75-3.612z" />
                    <path strokeLinecap="round" strokeLinejoin="round" d="M15 3v3.75a.75.75 0 0 0 .75.75H18" />
                  </svg>
                  <span className="text-sm">Copy Transcript</span>
                </button>
                {transcripts?.length > 0 && (
                  <>
                    <button
                      onClick={() => {
                        Analytics.trackButtonClick('generate_summary', 'meeting_details');
                        handleGenerateSummary(customPrompt);
                      }}
                      disabled={summaryStatus === 'processing'}
                      className={`px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm ${
                        summaryStatus === 'processing'
                          ? 'bg-yellow-50 border-yellow-200 text-yellow-700'
                          : 'bg-green-50 border-green-200 text-green-700 hover:bg-green-100 hover:border-green-300 active:bg-green-200'
                      }`}
                      title={
                        summaryStatus === 'processing'
                          ? 'Generating summary...'
                          : 'Generate AI Summary'
                      }
                    >
                      {summaryStatus === 'processing' ? (
                        <>
                          <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                          </svg>
                          <span className="text-sm">Processing...</span>
                        </>
                      ) : (
                        <>
                          <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <path strokeLinecap="round" strokeLinejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" />
                          </svg>
                          <span className="text-sm">Generate Note</span>
                        </>
                      )}
                    </button>
                    <Dialog>
                      <DialogTrigger asChild>
                        <button
                        className="px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm bg-gray-50 border-gray-200 text-gray-700 hover:bg-gray-100 hover:border-gray-300 active:bg-gray-200"
                        title="Model Settings"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                            <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          </svg>
                        </button>
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
              <div className="flex items-center gap-2">
                <button
                  onClick={() => {
                    Analytics.trackButtonClick('save_changes', 'meeting_details');
                    saveAllChanges();
                  }}
                  disabled={isSaving}
                  className={`px-3 py-1.5 rounded-md transition-all duration-200 flex items-center gap-1.5 text-sm ${isSaving ? 'bg-gray-200 text-gray-500' : 'bg-blue-500 hover:bg-blue-600 text-white'}`}
                >
                  {isSaving ? (
                    <>
                      <svg className="animate-spin h-4 w-4 text-gray-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                      </svg>
                      <span>Saving...</span>
                    </>
                  ) : (
                    <>
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" />
                      </svg>
                      <span>Save Changes</span>
                    </>
                  )}
                </button>
                {saveSuccess === true && (
                  <span className="text-green-500 flex items-center gap-1 text-sm">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                    Saved
                  </span>
                )}
                {saveSuccess === false && (
                  <span className="text-red-500 flex items-center gap-1 text-sm">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                    Failed to save
                  </span>
                )}
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
              <div className="flex-1 overflow-y-auto p-4">
                <AISummary 
                  summary={aiSummary} 
                  status={summaryStatus} 
                  error={summaryError}
                  onSummaryChange={(newSummary) => setAiSummary(newSummary)}
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
                <div className={`mt-4 p-4 rounded-lg ${
                  summaryStatus === 'error' ? 'bg-red-100 text-red-700' :
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



