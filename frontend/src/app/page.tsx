'use client';

import { useState, useEffect, useContext, useCallback, useRef } from 'react';
import { Transcript, TranscriptUpdate, Summary, SummaryResponse } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { TranscriptView } from '@/components/TranscriptView';
import { RecordingControls } from '@/components/RecordingControls';
import { AISummary } from '@/components/AISummary';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { listen } from '@tauri-apps/api/event';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { downloadDir } from '@tauri-apps/api/path';
import { listenerCount } from 'process';
import { invoke } from '@tauri-apps/api/core';
import { useNavigation } from '@/hooks/useNavigation';
import { useRouter } from 'next/navigation';
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import Analytics from '@/lib/analytics';



interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude';
  model: string;
  whisperModel: string;
}

type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

interface OllamaModel {
  name: string;
  id: string;
  size: string;
  modified: string;
}

export default function Home() {
  const [isRecording, setIsRecordingState] = useState(false);
  const [transcripts, setTranscripts] = useState<Transcript[]>([]);
  const [showSummary, setShowSummary] = useState(false);
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [barHeights, setBarHeights] = useState(['58%', '76%', '58%']);
  const [meetingTitle, setMeetingTitle] = useState('+ New Call');
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [customPrompt, setCustomPrompt] = useState('');
  const [aiSummary, setAiSummary] = useState<Summary | null>({
    key_points: { title: "Key Points", blocks: [] },
    action_items: { title: "Action Items", blocks: [] },
    decisions: { title: "Decisions", blocks: [] },
    main_topics: { title: "Main Topics", blocks: [] }
  });
  const [summaryResponse, setSummaryResponse] = useState<SummaryResponse | null>(null);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: 'llama3.2:latest',
    whisperModel: 'large-v3'
  });
  const [originalTranscript, setOriginalTranscript] = useState<string>('');
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [error, setError] = useState<string>('');
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [showErrorAlert, setShowErrorAlert] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');
  const [showChunkDropWarning, setShowChunkDropWarning] = useState(false);
  const [chunkDropMessage, setChunkDropMessage] = useState('');
  const [isSavingTranscript, setIsSavingTranscript] = useState(false);
  const [isRecordingDisabled, setIsRecordingDisabled] = useState(false);

  const { setCurrentMeeting, setMeetings, meetings, isMeetingActive, setIsMeetingActive, setIsRecording: setSidebarIsRecording , serverAddress} = useSidebar();
  const handleNavigation = useNavigation('', ''); // Initialize with empty values
  const router = useRouter();
  
  // Ref for final buffer flush functionality
  const finalFlushRef = useRef<(() => void) | null>(null);
  
  // Ref to avoid stale closure issues with transcripts
  const transcriptsRef = useRef<Transcript[]>(transcripts);
  
  // Keep ref updated with current transcripts
  useEffect(() => {
    transcriptsRef.current = transcripts;
  }, [transcripts]);

  const modelOptions = {
    ollama: models.map(model => model.name),
    claude: ['claude-3-5-sonnet-latest'],
    groq: ['llama-3.3-70b-versatile'],
  };

  useEffect(() => {
    if (models.length > 0 && modelConfig.provider === 'ollama') {
      setModelConfig(prev => ({
        ...prev,
        model: models[0].name
      }));
    }
  }, [models]);

  const whisperModels = [
    'tiny',
    'tiny.en',
    'tiny-q5_1',
    'tiny.en-q5_1',
    'tiny-q8_0',
    'base',
    'base.en',
    'base-q5_1',
    'base.en-q5_1',
    'base-q8_0',
    'small',
    'small.en',
    'small.en-tdrz',
    'small-q5_1',
    'small.en-q5_1',
    'small-q8_0',
    'medium',
    'medium.en',
    'medium-q5_0',
    'medium.en-q5_0',
    'medium-q8_0',
    'large-v1',
    'large-v2',
    'large-v2-q5_0',
    'large-v2-q8_0',
    'large-v3',
    'large-v3-q5_0',
    'large-v3-turbo',
    'large-v3-turbo-q5_0',
    'large-v3-turbo-q8_0'
  ];

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  useEffect(() => {
    setCurrentMeeting({ id: 'intro-call', title: meetingTitle });
    
  }, [meetingTitle, setCurrentMeeting]);

  useEffect(() => {
    const checkRecordingState = async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const isCurrentlyRecording = await invoke('is_recording');
        
        if (isCurrentlyRecording && !isRecording) {
          console.log('Recording is active in backend but not in UI, synchronizing state...');
          setIsRecordingState(true);
          setIsMeetingActive(true);
        } else if (!isCurrentlyRecording && isRecording) {
          console.log('Recording is inactive in backend but active in UI, synchronizing state...');
          setIsRecordingState(false);
        }
      } catch (error) {
        console.error('Failed to check recording state:', error);
      }
    };

    checkRecordingState();
    
    // Set up a polling interval to periodically check recording state
    const interval = setInterval(checkRecordingState, 1000); // Check every 1 second
    
    return () => clearInterval(interval);
  }, [isRecording, setIsMeetingActive]);
  


  useEffect(() => {
    if (isRecording) {
      const interval = setInterval(() => {
        setBarHeights(prev => {
          const newHeights = [...prev];
          newHeights[0] = Math.random() * 20 + 10 + 'px';
          newHeights[1] = Math.random() * 20 + 10 + 'px';
          newHeights[2] = Math.random() * 20 + 10 + 'px';
          return newHeights;
        });
      }, 300);

      return () => clearInterval(interval);
    }
  }, [isRecording]);
  
  // Update sidebar recording state when local recording state changes
  useEffect(() => {
    setSidebarIsRecording(isRecording);
  }, [isRecording, setSidebarIsRecording]);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;
    let transcriptCounter = 0;
    let transcriptBuffer = new Map<number, Transcript>();
    let lastProcessedSequence = 0;
    let processingTimer: NodeJS.Timeout | undefined;

    const processBufferedTranscripts = (forceFlush = false) => {
      const sortedTranscripts: Transcript[] = [];
      
      // Process all available sequential transcripts
      let nextSequence = lastProcessedSequence + 1;
      while (transcriptBuffer.has(nextSequence)) {
        const bufferedTranscript = transcriptBuffer.get(nextSequence)!;
        sortedTranscripts.push(bufferedTranscript);
        transcriptBuffer.delete(nextSequence);
        lastProcessedSequence = nextSequence;
        nextSequence++;
      }

      // Add any buffered transcripts that might be out of order
      const now = Date.now();
      const staleThreshold = 5000; // 5 seconds
      const recentThreshold = 2000; // 2 seconds - for recent out-of-order transcripts
      const staleTranscripts: Transcript[] = [];
      const recentTranscripts: Transcript[] = [];
      const forceFlushTranscripts: Transcript[] = [];
      
      for (const [sequenceId, transcript] of transcriptBuffer.entries()) {
        if (forceFlush) {
          // Force flush mode: process ALL remaining transcripts regardless of timing
          forceFlushTranscripts.push(transcript);
          transcriptBuffer.delete(sequenceId);
          console.log(`Force flush: processing transcript with sequence_id ${sequenceId}`);
        } else {
          const transcriptAge = now - parseInt(transcript.id.split('-')[0]);
          if (transcriptAge > staleThreshold) {
            // Process truly stale transcripts (>5s old)
            staleTranscripts.push(transcript);
            transcriptBuffer.delete(sequenceId);
          } else if (transcriptAge > recentThreshold) {
            // Process recent out-of-order transcripts (2-5s old)
            recentTranscripts.push(transcript);
            transcriptBuffer.delete(sequenceId);
            console.log(`Processing recent out-of-order transcript with sequence_id ${sequenceId}, age: ${transcriptAge}ms`);
          }
        }
      }
      
      // Sort both stale and recent transcripts by chunk_start_time, then by sequence_id
      const sortTranscripts = (transcripts: Transcript[]) => {
        return transcripts.sort((a, b) => {
          const chunkTimeDiff = (a.chunk_start_time || 0) - (b.chunk_start_time || 0);
          if (chunkTimeDiff !== 0) return chunkTimeDiff;
          return (a.sequence_id || 0) - (b.sequence_id || 0);
        });
      };

      const sortedStaleTranscripts = sortTranscripts(staleTranscripts);
      const sortedRecentTranscripts = sortTranscripts(recentTranscripts);
      const sortedForceFlushTranscripts = sortTranscripts(forceFlushTranscripts);

      const allNewTranscripts = [...sortedTranscripts, ...sortedRecentTranscripts, ...sortedStaleTranscripts, ...sortedForceFlushTranscripts];
      
      if (allNewTranscripts.length > 0) {
        setTranscripts(prev => {
          // Create a set of existing sequence_ids for deduplication
          const existingSequenceIds = new Set(prev.map(t => t.sequence_id).filter(id => id !== undefined));
          
          // Filter out any new transcripts that already exist
          const uniqueNewTranscripts = allNewTranscripts.filter(transcript => 
            transcript.sequence_id !== undefined && !existingSequenceIds.has(transcript.sequence_id)
          );
          
          // Only combine if we have unique new transcripts
          if (uniqueNewTranscripts.length === 0) {
            console.log('No unique transcripts to add - all were duplicates');
            return prev; // No new unique transcripts to add
          }
          
          console.log(`Adding ${uniqueNewTranscripts.length} unique transcripts out of ${allNewTranscripts.length} received`);
          
          // Merge with existing transcripts, maintaining chronological order
          const combined = [...prev, ...uniqueNewTranscripts];
          
          // Sort by chunk_start_time first, then by sequence_id
          return combined.sort((a, b) => {
            const chunkTimeDiff = (a.chunk_start_time || 0) - (b.chunk_start_time || 0);
            if (chunkTimeDiff !== 0) return chunkTimeDiff;
            return (a.sequence_id || 0) - (b.sequence_id || 0);
          });
        });
        
        // Log the processing summary
        const logMessage = forceFlush 
          ? `Force flush processed ${allNewTranscripts.length} transcripts (${sortedTranscripts.length} sequential, ${forceFlushTranscripts.length} forced)`
          : `Processed ${allNewTranscripts.length} transcripts (${sortedTranscripts.length} sequential, ${recentTranscripts.length} recent, ${staleTranscripts.length} stale)`;
        console.log(logMessage);
      }
    };

    // Assign final flush function to ref for external access
    finalFlushRef.current = () => processBufferedTranscripts(true);

    const setupListener = async () => {
      try {
        console.log('🔥 Setting up MAIN transcript listener during component initialization...');
        unlistenFn = await listen<TranscriptUpdate>('transcript-update', (event) => {
          const now = Date.now();
          console.log('🎯 MAIN LISTENER: Received transcript update:', {
            sequence_id: event.payload.sequence_id,
            text: event.payload.text.substring(0, 50) + '...',
            timestamp: event.payload.timestamp,
            is_partial: event.payload.is_partial,
            received_at: new Date(now).toISOString(),
            buffer_size_before: transcriptBuffer.size
          });
          
          // Check for duplicate sequence_id before processing
          if (transcriptBuffer.has(event.payload.sequence_id)) {
            console.log('🚫 MAIN LISTENER: Duplicate sequence_id, skipping buffer:', event.payload.sequence_id);
            return;
          }

          // Create transcript for buffer
          const newTranscript: Transcript = {
            id: `${Date.now()}-${transcriptCounter++}`,
            text: event.payload.text,
            timestamp: event.payload.timestamp,
            sequence_id: event.payload.sequence_id,
            chunk_start_time: event.payload.chunk_start_time,
            is_partial: event.payload.is_partial,
          };

          // Add to buffer
          transcriptBuffer.set(event.payload.sequence_id, newTranscript);
          console.log(`✅ MAIN LISTENER: Buffered transcript with sequence_id ${event.payload.sequence_id}. Buffer size: ${transcriptBuffer.size}, Last processed: ${lastProcessedSequence}`);

          // Clear any existing timer and set a new one
          if (processingTimer) {
            clearTimeout(processingTimer);
          }
          
          // Process buffer after a short delay to allow for batching
          processingTimer = setTimeout(processBufferedTranscripts, 100);
        });
        console.log('✅ MAIN transcript listener setup complete');
      } catch (error) {
        console.error('❌ Failed to setup MAIN transcript listener:', error);
        alert('Failed to setup transcript listener. Check console for details.');
      }
    };

    setupListener();
    console.log('Started enhanced listener setup');

    return () => {
      console.log('🧹 CLEANUP: Cleaning up MAIN transcript listener...');
      if (processingTimer) {
        clearTimeout(processingTimer);
        console.log('🧹 CLEANUP: Cleared processing timer');
      }
      if (unlistenFn) {
        unlistenFn();
        console.log('🧹 CLEANUP: MAIN transcript listener cleaned up');
      }
    };
  }, []);

  // Set up chunk drop warning listener
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupChunkDropListener = async () => {
      try {
        console.log('Setting up chunk-drop-warning listener...');
        unlistenFn = await listen<string>('chunk-drop-warning', (event) => {
          console.log('Chunk drop warning received:', event.payload);
          setChunkDropMessage(event.payload);
          setShowChunkDropWarning(true);
          
          // // Auto-dismiss after 8 seconds
          // setTimeout(() => {
          //   setShowChunkDropWarning(false);
          // }, 8000);
        });
        console.log('Chunk drop warning listener setup complete');
      } catch (error) {
        console.error('Failed to setup chunk drop warning listener:', error);
      }
    };

    setupChunkDropListener();

    return () => {
      console.log('Cleaning up chunk drop warning listener...');
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  useEffect(() => {
    const loadModels = async () => {
      try {
        const response = await fetch('http://localhost:11434/api/tags', {
          method: 'GET',
          headers: {
            'Content-Type': 'application/json',
          },
        });

        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`);
        }

        const data = await response.json();
        const modelList = data.models.map((model: any) => ({
          name: model.name,
          id: model.model,
          size: formatSize(model.size),
          modified: model.modified_at
        }));
        setModels(modelList);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load Ollama models');
        console.error('Error loading models:', err);
      }
    };

    loadModels();
  }, []);

  const formatSize = (size: number): string => {
    if (size < 1024) {
      return `${size} B`;
    } else if (size < 1024 * 1024) {
      return `${(size / 1024).toFixed(1)} KB`;
    } else if (size < 1024 * 1024 * 1024) {
      return `${(size / (1024 * 1024)).toFixed(1)} MB`;
    } else {
      return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`;
    }
  };

  const handleRecordingStart = async () => {
    try {
      console.log('Starting recording...');
      const { invoke } = await import('@tauri-apps/api/core');
      const randomTitle = `Meeting ${Math.random().toString(36).substring(2, 8)}`;
      setMeetingTitle(randomTitle);
      
      // Only check if we're already recording, but don't try to stop it first
      const isCurrentlyRecording = await invoke('is_recording');
      if (isCurrentlyRecording) {
        console.log('Already recording, cannot start a new recording');
        return; // Just return without starting a new recording
      }

      // Start new recording with whisper model
      await invoke('start_recording', {
        args: {
          whisper_model: modelConfig.whisperModel
        }
      });
      console.log('Recording started successfully');
      setIsRecordingState(true); // This will also update the sidebar via the useEffect
      setTranscripts([]); // Clear previous transcripts when starting new recording
      setIsMeetingActive(true);
      Analytics.trackButtonClick('start_recording', 'home_page');
    } catch (error) {
      console.error('Failed to start recording:', error);
      alert('Failed to start recording. Check console for details.');
      setIsRecordingState(false); // Reset state on error
      Analytics.trackButtonClick('start_recording_error', 'home_page');
    }
  };
  
  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isMeetingActive) {
          console.log('Auto-starting recording from navigation...');
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag
          await handleRecordingStart();
        }
      }
    };
    
    checkAutoStartRecording();
  }, [isRecording, isMeetingActive]);

  const handleRecordingStop = async () => {
    try {
      console.log('Stopping recording...');
      const { invoke } = await import('@tauri-apps/api/core');
      const { appDataDir } = await import('@tauri-apps/api/path');
      
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const transcriptPath = `${dataDir}transcript-${timestamp}.txt`;
      const audioPath = `${dataDir}recording-${timestamp}.wav`;

      // Stop recording and save audio
      await invoke('stop_recording', { 
        args: { 
          save_path: audioPath,
          model_config: modelConfig
        }
      });
      console.log('Recording stopped successfully');

      // Format and save transcript
      const formattedTranscript = transcripts
        .sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime())
        .map(t => `[${t.timestamp}] ${t.text}`)
        .join('\n\n');

      // const documentContent = `Meeting Title: ${meetingTitle}\nDate: ${new Date().toLocaleString()}\n\nTranscript:\n${formattedTranscript}`;

      // await invoke('save_transcript', { 
      //   filePath: transcriptPath,
      //   content: documentContent
      // });
      // console.log('Transcript saved to:', transcriptPath);

      setIsRecordingState(false);
      
      // Show summary button if we have transcript content
      if (formattedTranscript.trim()) {
        setShowSummary(true);
      } else {
        console.log('No transcript content available');
      }
    } catch (error) {
      console.error('Failed to stop recording:', error);
      if (error instanceof Error) {
        console.error('Error details:', {
          message: error.message,
          name: error.name,
          stack: error.stack,
        });
      }
      alert('Failed to stop recording. Check console for details.');
      setIsRecordingState(false); // Reset state on error
    }
  };

  const handleRecordingStop2 = async (isCallApi: boolean) => {
    setIsRecordingDisabled(true);
    const stopStartTime = Date.now();
    try {
      console.log('Stopping recording (new implementation)...', {
        stop_initiated_at: new Date(stopStartTime).toISOString(),
        current_transcript_count: transcripts.length
      });
      const { invoke } = await import('@tauri-apps/api/core');
      const { appDataDir } = await import('@tauri-apps/api/path');
      const { listen } = await import('@tauri-apps/api/event');
      
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const transcriptPath = `${dataDir}transcript-${timestamp}.txt`;
      const audioPath = `${dataDir}recording-${timestamp}.wav`;
      
      // Stop recording and get audio path
      await invoke('stop_recording', { 
        args: { 
          model_config: modelConfig,
          save_path: audioPath
        }
      });
      console.log('Recording stopped successfully');
      
      // Wait for transcription to complete
      setSummaryStatus('processing');
      console.log('Waiting for transcription to complete...');
      
      const MAX_WAIT_TIME = 60000; // 60 seconds maximum wait (increased for longer processing)
      const POLL_INTERVAL = 500; // Check every 500ms
      let elapsedTime = 0;
      let transcriptionComplete = false;
      
      // Listen for transcription-complete event
      const unlistenComplete = await listen('transcription-complete', () => {
        console.log('Received transcription-complete event');
        transcriptionComplete = true;
      });
      
      // Removed LATE transcript listener - relying on main buffered transcript system instead
      
      // Poll for transcription status
      while (elapsedTime < MAX_WAIT_TIME && !transcriptionComplete) {
        try {
          const status = await invoke<{chunks_in_queue: number, is_processing: boolean, last_activity_ms: number}>('get_transcription_status');
          console.log('Transcription status:', status);
          
          // Check if transcription is complete
          if (!status.is_processing && status.chunks_in_queue === 0) {
            console.log('Transcription complete - no active processing and no chunks in queue');
            transcriptionComplete = true;
            break;
          }
          
          // If no activity for more than 8 seconds and no chunks in queue, consider it done (increased from 5s to 8s)
          if (status.last_activity_ms > 8000 && status.chunks_in_queue === 0) {
            console.log('Transcription likely complete - no recent activity and empty queue');
            transcriptionComplete = true;
            break;
          }
          
          // Update user with current status
          if (status.chunks_in_queue > 0) {
            console.log(`Processing ${status.chunks_in_queue} remaining audio chunks...`);
            setSummaryStatus('processing');
          }
          
          // Wait before next check
          await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
          elapsedTime += POLL_INTERVAL;
        } catch (error) {
          console.error('Error checking transcription status:', error);
          break;
        }
      }
      
      // Clean up listener
      console.log('🧹 CLEANUP: Cleaning up transcription-complete listener');
      unlistenComplete();
      
      if (!transcriptionComplete && elapsedTime >= MAX_WAIT_TIME) {
        console.warn('⏰ Transcription wait timeout reached after', elapsedTime, 'ms');
      } else {
        console.log('✅ Transcription completed after', elapsedTime, 'ms');
        // Wait longer for any late transcript segments (increased from 1s to 4s)
        console.log('⏳ Waiting for late transcript segments...');
        await new Promise(resolve => setTimeout(resolve, 4000));
      }
      
      // LATE transcript listener removed - no cleanup needed
      
      // Final buffer flush: process ALL remaining transcripts regardless of timing
      const flushStartTime = Date.now();
      console.log('🔄 Final buffer flush: forcing processing of any remaining transcripts...', {
        flush_started_at: new Date(flushStartTime).toISOString(),
        time_since_stop: flushStartTime - stopStartTime,
        current_transcript_count: transcripts.length
      });
      if (finalFlushRef.current) {
        finalFlushRef.current();
        const flushEndTime = Date.now();
        console.log('✅ Final buffer flush completed', {
          flush_duration: flushEndTime - flushStartTime,
          total_time_since_stop: flushEndTime - stopStartTime,
          final_transcript_count: transcripts.length
        });
      } else {
        console.log('⚠️ Final flush function not available');
      }
      
      setSummaryStatus('idle');

      // Wait a bit more to ensure all transcript state updates have been processed
      console.log('Waiting for transcript state updates to complete...');
      await new Promise(resolve => setTimeout(resolve, 500));

      // Save to SQLite
      if (isCallApi && transcriptionComplete == true) {

        // await new Promise(resolve => setTimeout(resolve, 5000));
        setIsSavingTranscript(true);

        // Fix stale closure issue: Use ref to get fresh transcript state
        console.log('🔄 Solving stale closure - getting fresh transcript state at save time...');
        
        // // Force final buffer flush to capture any remaining transcripts
        // if (finalFlushRef.current) {
        //   finalFlushRef.current();
        // }
        
        // // Wait a moment for any final state updates to propagate
        // await new Promise(resolve => setTimeout(resolve, 300));
        
        // Get fresh transcript state using ref (avoids stale closure)
        const freshTranscripts = [...transcriptsRef.current];
        
        console.log('💾 Saving transcript to database with fresh state...', {
          fresh_transcript_count: freshTranscripts.length,
          sample_text: freshTranscripts.length > 0 ? freshTranscripts[0].text.substring(0, 50) + '...' : 'none',
          last_transcript: freshTranscripts.length > 0 ? freshTranscripts[freshTranscripts.length - 1].text.substring(0, 30) + '...' : 'none'
        });
        
        const responseData = await invoke('api_save_transcript', {
          meetingTitle: meetingTitle,
          transcripts: freshTranscripts, // Use fresh state, not stale closure
        }) as any;

        const meetingId = responseData.meeting_id;
        if (!meetingId) {
          console.error('No meeting_id in response:', responseData);
          throw new Error('No meeting ID received from save operation');
        }
        
        console.log('Successfully saved transcript with meeting ID:', meetingId);
        setMeetings([{ id: meetingId, title: meetingTitle }, ...meetings]);
        
        // Wait a moment to ensure backend has fully processed the save
        console.log('Waiting for backend processing to complete...');
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        // Set current meeting and navigate
        console.log('Setting current meeting and navigating to details page');
        setCurrentMeeting({ id: meetingId, title: meetingTitle });
        setIsMeetingActive(false);
        router.push('/meeting-details');
      }
      setIsMeetingActive(false);
      setIsRecordingState(false);
      setIsRecordingDisabled(false);
      // Show summary button if we have transcript content
      if (transcripts.length > 0) {
        setShowSummary(true);
      } else {
        console.log('No transcript content available');
      }
    } catch (error) {
      console.error('Error in handleRecordingStop2:', error);
      setIsRecordingState(false);
      setSummaryStatus('idle');
      setIsSavingTranscript(false);
      setIsRecordingDisabled(false);
    }
  };

  const handleTranscriptUpdate = (update: any) => {
    console.log('🎯 handleTranscriptUpdate called with:', {
      sequence_id: update.sequence_id,
      text: update.text.substring(0, 50) + '...',
      timestamp: update.timestamp,
      is_partial: update.is_partial
    });
    
    const newTranscript = {
      id: update.sequence_id ? update.sequence_id.toString() : Date.now().toString(),
      text: update.text,
      timestamp: update.timestamp,
      sequence_id: update.sequence_id || 0,
    };
    
    setTranscripts(prev => {
      console.log('📊 Current transcripts count before update:', prev.length);
      
      // Check if this transcript already exists
      const exists = prev.some(
        t => t.text === update.text && t.timestamp === update.timestamp
      );
      if (exists) {
        console.log('🚫 Duplicate transcript detected, skipping:', update.text.substring(0, 30) + '...');
        return prev;
      }
      
      // Add new transcript and sort by sequence_id to maintain order
      const updated = [...prev, newTranscript];
      const sorted = updated.sort((a, b) => (a.sequence_id || 0) - (b.sequence_id || 0));
      
      console.log('✅ Added new transcript. New count:', sorted.length);
      console.log('📝 Latest transcript:', {
        id: newTranscript.id,
        text: newTranscript.text.substring(0, 30) + '...',
        sequence_id: newTranscript.sequence_id
      });
      
      return sorted;
    });
  };

  const generateAISummary = useCallback(async (prompt: string = '') => {
    setSummaryStatus('processing');
    setSummaryError(null);

    try {
      const fullTranscript = transcripts.map(t => t.text).join('\n');
      if (!fullTranscript.trim()) {
        throw new Error('No transcript text available. Please add some text first.');
      }
      
      // Store the original transcript for regeneration
      setOriginalTranscript(fullTranscript);
      
      console.log('Generating summary for transcript length:', fullTranscript.length);
      
      // Process transcript and get process_id
      console.log('Processing transcript...');
      const result = await invoke('api_process_transcript', {
        text: fullTranscript,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        chunkSize: 40000,
        overlap: 1000,
        customPrompt: prompt,
      }) as any;

      const process_id = result.process_id;
      console.log('Process ID:', process_id);
   

      // Poll for summary status
      const pollInterval = setInterval(async () => {
        try {
          const result = await invoke('api_get_summary', {
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
            }

            // Format the summary data with consistent styling
            const formattedSummary = Object.entries(summaryData).reduce((acc: Summary, [key, section]: [string, any]) => {
              acc[key] = {
                title: section.title,
                blocks: section.blocks.map((block: any) => ({
                  ...block,
                  // type: 'bullet',
                  color: 'default',
                  content: block.content.trim() // Remove trailing newlines
                }))
              };
              return acc;
            }, {} as Summary);

            setAiSummary(formattedSummary);
            setSummaryStatus('completed');
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
        }
      }, 3000); // Poll every 3 seconds

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
    }
  }, [transcripts, modelConfig]);

  const handleSummary = useCallback((summary: any) => {
    setAiSummary(summary);
  }, []);

  const handleSummaryChange = (newSummary: Summary) => {
    console.log('Summary changed:', newSummary);
    setAiSummary(newSummary);
  };

  const handleTitleChange = (newTitle: string) => {
    setMeetingTitle(newTitle);
    setCurrentMeeting({ id: 'intro-call', title: newTitle });
  };

  const getSummaryStatusMessage = (status: SummaryStatus) => {
    switch (status) {
      case 'idle':
        return 'Ready to generate summary';
      case 'processing':
        return isRecording ? 'Processing transcript...' : 'Finalizing transcription...';
      case 'summarizing':
        return 'Generating AI summary...';
      case 'regenerating':
        return 'Regenerating AI summary...';
      case 'completed':
        return 'Summary generated successfully!';
      case 'error':
        return summaryError || 'An error occurred';
      default:
        return '';
    }
  };

  const handleDownloadTranscript = async () => {
    try {
      // Create transcript object with metadata
      const transcriptData = {
        title: meetingTitle,
        timestamp: new Date().toISOString(),
        transcripts: transcripts
      };

      // Generate filename
      const sanitizedTitle = meetingTitle.replace(/[^a-zA-Z0-9]/g, '_');
      const filename = `${sanitizedTitle}_transcript.json`;
      
      // Get download directory path
      const downloadPath = await downloadDir();
      
      // Write file to downloads directory
      await writeTextFile(`${downloadPath}/${filename}`, JSON.stringify(transcriptData, null, 2));

      console.log('Transcript saved successfully to:', `${downloadPath}/${filename}`);
      alert('Transcript downloaded successfully!');
    } catch (error) {
      console.error('Failed to save transcript:', error);
      alert('Failed to save transcript. Please try again.');
    }
  };

  const handleUploadTranscript = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    try {
      const text = await file.text();
      const data = JSON.parse(text);
      
      // Validate the uploaded file structure
      if (!data.transcripts || !Array.isArray(data.transcripts)) {
        throw new Error('Invalid transcript file format');
      }

      // Update state with uploaded data
      setMeetingTitle(data.title || 'Uploaded Transcript');
      setTranscripts(data.transcripts);
      
      // Generate summary for the uploaded transcript
      handleSummary(data.transcripts);
    } catch (error) {
      console.error('Error uploading transcript:', error);
      alert('Failed to upload transcript. Please make sure the file format is correct.');
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
      
      // Process transcript and get process_id
      console.log('Processing transcript...');
      const result = await invoke('api_process_transcript', {
        text: originalTranscript,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        chunkSize: 40000,
        overlap: 1000,
      }) as any;

      const process_id = result.process_id;
      console.log('Process ID:', process_id);

      // Poll for summary status
      const pollInterval = setInterval(async () => {
        try {
          const result = await invoke('api_get_summary', {
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
            }

            // Format the summary data with consistent styling
            const formattedSummary = Object.entries(summaryData).reduce((acc: Summary, [key, section]: [string, any]) => {
              acc[key] = {
                title: section.title,
                blocks: section.blocks.map((block: any) => ({
                  ...block,
                  // type: 'bullet',
                  color: 'default',
                  content: block.content.trim()
                }))
              };
              return acc;
            }, {} as Summary);

            setAiSummary(formattedSummary);
            setSummaryStatus('completed');
          } else if (result.status === 'error') {
            clearInterval(pollInterval);
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
        }
      }, 1000);

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
    }
  }, [originalTranscript, modelConfig]);

  const handleCopyTranscript = useCallback(() => {
    const fullTranscript = transcripts
      .map(t => `${t.timestamp}: ${t.text}`)
      .join('\n');
    navigator.clipboard.writeText(fullTranscript);
  }, [transcripts]);

  const handleGenerateSummary = useCallback(async () => {
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

  const isSummaryLoading = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

  return (
    <div className="flex flex-col h-screen bg-gray-50">
      {showErrorAlert && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <Alert className="max-w-md mx-4 border-red-200 bg-white shadow-xl">
            <AlertTitle className="text-red-800">Recording Stopped</AlertTitle>
            <AlertDescription className="text-red-700">
              {errorMessage}
              <button
                onClick={() => setShowErrorAlert(false)}
                className="ml-2 text-red-600 hover:text-red-800 underline"
              >
                Dismiss
              </button>
            </AlertDescription>
          </Alert>
        </div>
      )}
      {showChunkDropWarning && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <Alert className="max-w-lg mx-4 border-yellow-200 bg-white shadow-xl">
            <AlertTitle className="text-yellow-800">Transcription Performance Warning</AlertTitle>
            <AlertDescription className="text-yellow-700">
              {chunkDropMessage}
              <button
                onClick={() => setShowChunkDropWarning(false)}
                className="ml-2 text-yellow-600 hover:text-yellow-800 underline"
              >
                Dismiss
              </button>
            </AlertDescription>
          </Alert>
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        {/* Left side - Transcript */}
        <div className="w-1/3 min-w-[300px] border-r border-gray-200 bg-white flex flex-col relative">
          {/* Title area */}
          <div className="p-4 border-b border-gray-200">
            <div className="flex flex-col space-y-3">
              <div className="flex flex-col space-y-2">
                <div className="flex items-center space-x-2">
                  <button
                    onClick={handleCopyTranscript}
                    disabled={transcripts.length === 0}
                    className={`px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm ${
                      transcripts.length === 0
                        ? 'bg-gray-50 border-gray-200 text-gray-400 cursor-not-allowed'
                        : 'bg-blue-50 border-blue-200 text-blue-700 hover:bg-blue-100 hover:border-blue-300 active:bg-blue-200'
                    }`}
                    title={transcripts.length === 0 ? 'No transcript available' : 'Copy Transcript'}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" strokeWidth="2" stroke="currentColor" fill="none">
                      <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 3h-6a2.25 2.25 0 0 0-2.25 2.25v13.5A2.25 2.25 0 0 0 7.5 21h6a2.25 2.25 0 0 0 2.25-2.25V7.5l-3.75-3.612z" />
                      <path strokeLinecap="round" strokeLinejoin="round" d="M15 3v3.75a.75.75 0 0 0 .75.75H18" />
                    </svg>
                    <span className="text-sm">Copy Transcript</span>
                  </button>
                  {/* {showSummary && !isRecording && (
                    <>
                      <button
                        onClick={handleGenerateSummary}
                        disabled={summaryStatus === 'processing'}
                        className={`px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm ${
                          summaryStatus === 'processing'
                            ? 'bg-yellow-50 border-yellow-200 text-yellow-700'
                            : transcripts.length === 0
                            ? 'bg-gray-50 border-gray-200 text-gray-400 cursor-not-allowed'
                            : 'bg-green-50 border-green-200 text-green-700 hover:bg-green-100 hover:border-green-300 active:bg-green-200'
                        }`}
                        title={
                          summaryStatus === 'processing'
                            ? 'Generating summary...'
                            : transcripts.length === 0
                            ? 'No transcript available'
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
                      <button
                        onClick={() => setShowModelSettings(true)}
                        className="px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm bg-gray-50 border-gray-200 text-gray-700 hover:bg-gray-100 hover:border-gray-300 active:bg-gray-200"
                        title="Model Settings"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                          <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                      </button>
                    </>
                  )} */}
                </div>

                {/* {showSummary && !isRecording && (
                  <>
                    <button
                      onClick={handleGenerateSummary}
                      disabled={summaryStatus === 'processing'}
                      className={`px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm ${
                        summaryStatus === 'processing'
                          ? 'bg-yellow-50 border-yellow-200 text-yellow-700'
                          : transcripts.length === 0
                          ? 'bg-gray-50 border-gray-200 text-gray-400 cursor-not-allowed'
                          : 'bg-green-50 border-green-200 text-green-700 hover:bg-green-100 hover:border-green-300 active:bg-green-200'
                      }`}
                      title={
                        summaryStatus === 'processing'
                          ? 'Generating summary...'
                          : transcripts.length === 0
                          ? 'No transcript available'
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
                    <button
                      onClick={() => setShowModelSettings(true)}
                      className="px-3 py-2 border rounded-md transition-all duration-200 inline-flex items-center gap-2 shadow-sm bg-gray-50 border-gray-200 text-gray-700 hover:bg-gray-100 hover:border-gray-300 active:bg-gray-200"
                      title="Model Settings"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      </svg>
                    </button>
                  </>
                )} */}
              </div>
            </div>
          </div>

          {/* Transcript content */}
          <div className="flex-1 overflow-y-auto pb-32">
            <TranscriptView transcripts={transcripts} />
          </div>
          
          {/* Custom prompt input at bottom of transcript section */}
          {/* {!isRecording && transcripts.length > 0 && !isMeetingActive && (
            <div className="p-4 border-t border-gray-200">
              <textarea
                placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
                className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
                value={customPrompt}
                onChange={(e) => setCustomPrompt(e.target.value)}
                disabled={summaryStatus === 'processing'}
              />
            </div>
          )} */}

          {/* Recording controls */}
          <div className="absolute bottom-16 left-1/2 transform -translate-x-1/2 z-10">
            <div className="bg-white rounded-full shadow-lg flex items-center">
              <RecordingControls
                isRecording={isRecording}
                onRecordingStop={(callApi = true) => handleRecordingStop2(callApi)}
                onRecordingStart={handleRecordingStart}
                onTranscriptReceived={handleTranscriptUpdate}
                barHeights={barHeights}
                onTranscriptionError={(message) => {
                  setErrorMessage(message);
                  setShowErrorAlert(true);
                }}
                isRecordingDisabled={isRecordingDisabled}
              />
            </div>
          </div>

          {/* Processing status overlay */}
          {summaryStatus === 'processing' && !isRecording && (
            <div className="absolute bottom-32 left-1/2 transform -translate-x-1/2 z-10 bg-white rounded-lg shadow-lg px-4 py-2 flex items-center space-x-2">
              <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-gray-900"></div>
              <span className="text-sm text-gray-700">Finalizing transcription...</span>
            </div>
          )}
          {isSavingTranscript && (
            <div className="absolute bottom-32 left-1/2 transform -translate-x-1/2 z-10 bg-white rounded-lg shadow-lg px-4 py-2 flex items-center space-x-2">
              <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-gray-900"></div>
              <span className="text-sm text-gray-700">Saving transcript...</span>
            </div>
          )}

          {/* Model Settings Modal */}
          {showModelSettings && (
            <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
              <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4 shadow-xl">
                <div className="flex justify-between items-center mb-4">
                  <h3 className="text-lg font-semibold text-gray-900">Model Settings</h3>
                  <button
                    onClick={() => setShowModelSettings(false)}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                <div className="space-y-4">
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      Summarization Model
                    </label>
                    <div className="flex space-x-2">
                      <select
                        className="px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                        value={modelConfig.provider}
                        onChange={(e) => {
                          const provider = e.target.value as ModelConfig['provider'];
                          setModelConfig({
                            ...modelConfig,
                            provider,
                            model: modelOptions[provider][0]
                          });
                        }}
                      >
                        <option value="claude">Claude</option>
                        <option value="groq">Groq</option>
                        <option value="ollama">Ollama</option>
                      </select>

                      <select
                        className="flex-1 px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                        value={modelConfig.model}
                        onChange={(e) => setModelConfig(prev => ({ ...prev, model: e.target.value }))}
                      >
                        {modelOptions[modelConfig.provider].map(model => (
                          <option key={model} value={model}>
                            {model}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                  {modelConfig.provider === 'ollama' && (
                    <div>
                      <h4 className="text-lg font-bold mb-4">Available Ollama Models</h4>
                      {error && (
                        <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">
                          {error}
                        </div>
                      )}
                      <div className="grid gap-4 max-h-[400px] overflow-y-auto pr-2">
                        {models.map((model) => (
                          <div 
                            key={model.id}
                            className={`bg-white p-4 rounded-lg shadow cursor-pointer transition-colors ${
                              modelConfig.model === model.name ? 'ring-2 ring-blue-500 bg-blue-50' : 'hover:bg-gray-50'
                            }`}
                            onClick={() => setModelConfig(prev => ({ ...prev, model: model.name }))}
                          >
                            <h3 className="font-bold">{model.name}</h3>
                            <p className="text-gray-600">Size: {model.size}</p>
                            <p className="text-gray-600">Modified: {model.modified}</p>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>

                <div className="mt-6 flex justify-end">
                  <button
                    onClick={() => setShowModelSettings(false)}
                    className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
                  >
                    Done
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Right side - AI Summary */}
        <div className="flex-1 overflow-y-auto bg-white">
          <div className="p-4 border-b border-gray-200">
            <div className="flex items-center">
              <EditableTitle
                title={meetingTitle}
                isEditing={isEditingTitle}
                onStartEditing={() => setIsEditingTitle(true)}
                onFinishEditing={() => setIsEditingTitle(false)}
                onChange={handleTitleChange}
              />
            </div>
          </div>
          {/* {isSummaryLoading ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mb-4"></div>
                <p className="text-gray-600">Generating AI Summary...</p>
              </div>
            </div>
          ) : showSummary && (
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
                  onRegenerateSummary={handleRegenerateSummary}
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
          )} */}
        </div>
      </div>
    </div>
  );
}
