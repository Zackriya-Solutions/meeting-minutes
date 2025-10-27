'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { motion } from 'framer-motion';
import { Transcript, TranscriptUpdate } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { RecordingControls } from '@/components/RecordingControls';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { TranscriptSettings, TranscriptModelProps } from '@/components/TranscriptSettings';
import { LanguageSelection } from '@/components/LanguageSelection';
import { PermissionWarning } from '@/components/PermissionWarning';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { Button } from '@/components/ui/button';
import { Copy, GlobeIcon } from 'lucide-react';
import { toast } from 'sonner';
import { ButtonGroup } from '@/components/ui/button-group';



interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openrouter' | 'openai';
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
  // const [showSummary, setShowSummary] = useState(false);
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [barHeights, setBarHeights] = useState(['58%', '76%', '58%']);
  const [meetingTitle, setMeetingTitle] = useState('+ New Call');
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: 'llama3.2:latest',
    whisperModel: 'large-v3'
  });
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
    apiKey: null
  });
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [error, setError] = useState<string>('');
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [showErrorAlert, setShowErrorAlert] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');
  const [showChunkDropWarning, setShowChunkDropWarning] = useState(false);
  const [chunkDropMessage, setChunkDropMessage] = useState('');
  const [isSavingTranscript, setIsSavingTranscript] = useState(false);
  const [isRecordingDisabled, setIsRecordingDisabled] = useState(false);
  const [selectedDevices, setSelectedDevices] = useState<SelectedDevices>({
    micDevice: null,
    systemDevice: null
  });
  const [showDeviceSettings, setShowDeviceSettings] = useState(false);
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [modelSelectorMessage, setModelSelectorMessage] = useState('');
  const [showLanguageSettings, setShowLanguageSettings] = useState(false);
  const [selectedLanguage, setSelectedLanguage] = useState('auto-translate');
  const [isProcessingTranscript, setIsProcessingTranscript] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [showConfidenceIndicator, setShowConfidenceIndicator] = useState<boolean>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('showConfidenceIndicator');
      return saved !== null ? saved === 'true' : true;
    }
    return true;
  });

  // Permission check hook
  const { hasMicrophone, hasSystemAudio, isChecking: isCheckingPermissions, checkPermissions } = usePermissionCheck();

  // Recording state context - provides backend-synced state
  const recordingState = useRecordingState();

  const { setCurrentMeeting, setMeetings, meetings, isMeetingActive, setIsMeetingActive, setIsRecording: setSidebarIsRecording, isCollapsed: sidebarCollapsed, refetchMeetings } = useSidebar();
  const router = useRouter();

  // Ref for final buffer flush functionality
  const finalFlushRef = useRef<(() => void) | null>(null);

  // Ref to avoid stale closure issues with transcripts
  const transcriptsRef = useRef<Transcript[]>(transcripts);

  const isUserAtBottomRef = useRef<boolean>(true);

  // Ref for the transcript scrollable container
  const transcriptContainerRef = useRef<HTMLDivElement>(null);

  // Keep ref updated with current transcripts
  useEffect(() => {
    transcriptsRef.current = transcripts;
  }, [transcripts]);

  // Smart auto-scroll: Track user scroll position
  useEffect(() => {
    const handleScroll = () => {
      const container = transcriptContainerRef.current;
      if (!container) return;

      const { scrollTop, scrollHeight, clientHeight } = container;
      const isAtBottom = scrollTop + clientHeight >= scrollHeight - 10; // 10px tolerance
      isUserAtBottomRef.current = isAtBottom;
    };

    const container = transcriptContainerRef.current;
    if (container) {
      container.addEventListener('scroll', handleScroll);
      return () => container.removeEventListener('scroll', handleScroll);
    }
  }, []);

  // Auto-scroll when transcripts change (only if user is at bottom)
  useEffect(() => {
    // Only auto-scroll if user was at the bottom before new content
    if (isUserAtBottomRef.current && transcriptContainerRef.current) {
      // Wait for Framer Motion animation to complete (150ms) before scrolling
      // This ensures scrollHeight includes the full rendered height of the new transcript
      const scrollTimeout = setTimeout(() => {
        const container = transcriptContainerRef.current;
        if (container) {
          container.scrollTo({
            top: container.scrollHeight,
            behavior: 'smooth'
          });
        }
      }, 150); // Match Framer Motion transition duration

      return () => clearTimeout(scrollTimeout);
    }
  }, [transcripts]);

  const modelOptions = {
    ollama: models.map(model => model.name),
    claude: ['claude-3-5-sonnet-latest'],
    groq: ['llama-3.3-70b-versatile'],
    openrouter: [],
    openai: ['gpt-4', 'gpt-4-turbo', 'gpt-3.5-turbo'],
  };

  useEffect(() => {
    if (models.length > 0 && modelConfig.provider === 'ollama') {
      setModelConfig(prev => ({
        ...prev,
        model: models[0].name
      }));
    }
  }, [models]);

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          console.log('Loaded saved transcript config:', config);
          setTranscriptModelConfig({
            provider: config.provider || 'parakeet',
            model: config.model || 'parakeet-tdt-0.6b-v3-int8',
            apiKey: config.apiKey || null
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, []);

  useEffect(() => {
    setCurrentMeeting({ id: 'intro-call', title: meetingTitle });

  }, [meetingTitle, setCurrentMeeting]);

  useEffect(() => {
    console.log('Setting up recording state check effect, current isRecording:', isRecording);

    const checkRecordingState = async () => {
      try {
        console.log('checkRecordingState called');
        const { invoke } = await import('@tauri-apps/api/core');
        console.log('About to call is_recording command');
        const isCurrentlyRecording = await invoke('is_recording');
        console.log('checkRecordingState: backend recording =', isCurrentlyRecording, 'UI recording =', isRecording);

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

    // Test if Tauri is available
    console.log('Testing Tauri availability...');
    if (typeof window !== 'undefined' && (window as any).__TAURI__) {
      console.log('Tauri is available, starting state check');
      checkRecordingState();

      // Set up a polling interval to periodically check recording state
      const interval = setInterval(checkRecordingState, 1000); // Check every 1 second

      return () => {
        console.log('Cleaning up recording state check interval');
        clearInterval(interval);
      };
    } else {
      console.log('Tauri is not available, skipping state check');
    }
  }, [setIsMeetingActive]);



  useEffect(() => {
    if (recordingState.isRecording) {
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
  }, [recordingState.isRecording]);

  // Update sidebar recording state when backend-synced recording state changes
  useEffect(() => {
    setSidebarIsRecording(recordingState.isRecording);
  }, [recordingState.isRecording, setSidebarIsRecording]);

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
      const staleThreshold = 100;  // 100ms safety net only (serial workers = sequential order)
      const recentThreshold = 0;    // Show immediately - no delay needed with serial processing
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
            // Process stale transcripts (>100ms old - safety net)
            staleTranscripts.push(transcript);
            transcriptBuffer.delete(sequenceId);
          } else if (transcriptAge >= recentThreshold) {
            // Process immediately (0ms threshold with serial workers)
            recentTranscripts.push(transcript);
            transcriptBuffer.delete(sequenceId);
            console.log(`Processing transcript with sequence_id ${sequenceId}, age: ${transcriptAge}ms`);
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

          // Create transcript for buffer with NEW timestamp fields
          const newTranscript: Transcript = {
            id: `${Date.now()}-${transcriptCounter++}`,
            text: event.payload.text,
            timestamp: event.payload.timestamp,
            sequence_id: event.payload.sequence_id,
            chunk_start_time: event.payload.chunk_start_time,
            is_partial: event.payload.is_partial,
            confidence: event.payload.confidence,
            // NEW: Recording-relative timestamps for playback sync
            audio_start_time: event.payload.audio_start_time,
            audio_end_time: event.payload.audio_end_time,
            duration: event.payload.duration,
          };

          // Add to buffer
          transcriptBuffer.set(event.payload.sequence_id, newTranscript);
          console.log(`✅ MAIN LISTENER: Buffered transcript with sequence_id ${event.payload.sequence_id}. Buffer size: ${transcriptBuffer.size}, Last processed: ${lastProcessedSequence}`);

          // Clear any existing timer and set a new one
          if (processingTimer) {
            clearTimeout(processingTimer);
          }

          // Process buffer with minimal delay for immediate UI updates (serial workers = sequential order)
          processingTimer = setTimeout(processBufferedTranscripts, 10);
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

  // Sync transcript history and meeting name from backend on reload
  // This fixes the issue where reloading during active recording causes state desync
  useEffect(() => {
    const syncFromBackend = async () => {
      // Only sync if recording is active but we have no local transcripts
      if (recordingState.isRecording && transcripts.length === 0) {
        try {
          console.log('[Reload Sync] Recording active after reload, syncing transcript history...');

          // Fetch transcript history from backend
          const history = await invoke<any[]>('get_transcript_history');
          console.log(`[Reload Sync] Retrieved ${history.length} transcript segments from backend`);

          // Convert backend format to frontend Transcript format
          const formattedTranscripts: Transcript[] = history.map((segment: any) => ({
            id: segment.id,
            text: segment.text,
            timestamp: segment.display_time, // Use display_time for UI
            sequence_id: segment.sequence_id,
            chunk_start_time: segment.audio_start_time,
            is_partial: false, // History segments are always final
            confidence: segment.confidence,
            audio_start_time: segment.audio_start_time,
            audio_end_time: segment.audio_end_time,
            duration: segment.duration,
          }));

          setTranscripts(formattedTranscripts);
          console.log('[Reload Sync] ✅ Transcript history synced successfully');

          // Fetch meeting name from backend
          const meetingName = await invoke<string | null>('get_recording_meeting_name');
          if (meetingName) {
            console.log('[Reload Sync] Retrieved meeting name:', meetingName);
            setMeetingTitle(meetingName);
            console.log('[Reload Sync] ✅ Meeting title synced successfully');
          }
        } catch (error) {
          console.error('[Reload Sync] Failed to sync from backend:', error);
        }
      }
    };

    syncFromBackend();
  }, [recordingState.isRecording]); // Run when recording state changes

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

  // Set up recording-stopped listener for meeting navigation
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupRecordingStoppedListener = async () => {
      try {
        console.log('Setting up recording-stopped listener for navigation...');
        unlistenFn = await listen<{
          message: string;
          folder_path?: string;
          meeting_name?: string;
        }>('recording-stopped', async (event) => {
          console.log('Recording stopped event received:', event.payload);

          const { folder_path, meeting_name } = event.payload;

          // Store folder_path and meeting_name for later use in handleRecordingStop2
          if (folder_path) {
            sessionStorage.setItem('last_recording_folder_path', folder_path);
            console.log('✅ Stored folder_path for frontend save:', folder_path);
          }
          if (meeting_name) {
            sessionStorage.setItem('last_recording_meeting_name', meeting_name);
            console.log('✅ Stored meeting_name for frontend save:', meeting_name);
          }

        });
        console.log('Recording stopped listener setup complete');
      } catch (error) {
        console.error('Failed to setup recording stopped listener:', error);
      }
    };

    setupRecordingStoppedListener();

    return () => {
      console.log('Cleaning up recording stopped listener...');
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [router]);

  // Set up transcription error listener for model loading failures
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupTranscriptionErrorListener = async () => {
      try {
        console.log('Setting up transcription-error listener...');
        unlistenFn = await listen<{ error: string, userMessage: string, actionable: boolean }>('transcription-error', (event) => {
          console.log('Transcription error received:', event.payload);
          const { userMessage, actionable } = event.payload;

          if (actionable) {
            // This is a model-related error that requires user action
            setModelSelectorMessage(userMessage);
            setShowModelSelector(true);
          } else {
            // Regular transcription error
            setErrorMessage(userMessage);
            setShowErrorAlert(true);
          }
        });
        console.log('Transcription error listener setup complete');
      } catch (error) {
        console.error('Failed to setup transcription error listener:', error);
      }
    };

    setupTranscriptionErrorListener();

    return () => {
      console.log('Cleaning up transcription error listener...');
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
      console.log('handleRecordingStart called - setting up meeting title and state');

      const now = new Date();
      const day = String(now.getDate()).padStart(2, '0');
      const month = String(now.getMonth() + 1).padStart(2, '0');
      const year = String(now.getFullYear()).slice(-2);
      const hours = String(now.getHours()).padStart(2, '0');
      const minutes = String(now.getMinutes()).padStart(2, '0');
      const seconds = String(now.getSeconds()).padStart(2, '0');
      const randomTitle = `Meeting ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
      setMeetingTitle(randomTitle);

      // Update state - the actual recording is already started by RecordingControls
      console.log('Setting isRecordingState to true');
      setIsRecordingState(true); // This will also update the sidebar via the useEffect
      setTranscripts([]); // Clear previous transcripts when starting new recording
      setIsMeetingActive(true);
      Analytics.trackButtonClick('start_recording', 'home_page');

      // Show recording notification if enabled
      await showRecordingNotification();
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

          // Start the actual backend recording
          try {
            const { invoke } = await import('@tauri-apps/api/core');

            // Generate meeting title
            const now = new Date();
            const day = String(now.getDate()).padStart(2, '0');
            const month = String(now.getMonth() + 1).padStart(2, '0');
            const year = String(now.getFullYear()).slice(-2);
            const hours = String(now.getHours()).padStart(2, '0');
            const minutes = String(now.getMinutes()).padStart(2, '0');
            const seconds = String(now.getSeconds()).padStart(2, '0');
            const generatedMeetingTitle = `Meeting ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;

            console.log('Auto-starting backend recording with meeting:', generatedMeetingTitle);
            const result = await invoke('start_recording_with_devices_and_meeting', {
              mic_device_name: selectedDevices?.micDevice || null,
              system_device_name: selectedDevices?.systemDevice || null,
              meeting_name: generatedMeetingTitle
            });
            console.log('Auto-start backend recording result:', result);

            // Update UI state after successful backend start
            setMeetingTitle(generatedMeetingTitle);
            setIsRecordingState(true);
            setTranscripts([]);
            setIsMeetingActive(true);
            Analytics.trackButtonClick('start_recording', 'sidebar_auto');

            // Show recording notification if enabled
            await showRecordingNotification();
          } catch (error) {
            console.error('Failed to auto-start recording:', error);
            alert('Failed to start recording. Check console for details.');
            Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
          }
        }
      }
    };

    checkAutoStartRecording();
  }, [isRecording, isMeetingActive, selectedDevices]);

  const handleRecordingStop2 = async (isCallApi: boolean) => {
    // Immediately update UI state to reflect that recording has stopped
    // Note: setIsStopping(true) is now called via onStopInitiated callback before this function
    setIsRecordingState(false);
    setIsRecordingDisabled(true);
    setIsProcessingTranscript(true); // Immediately set processing flag for UX
    const stopStartTime = Date.now();
    try {
      console.log('Post-stop processing (new implementation)...', {
        stop_initiated_at: new Date(stopStartTime).toISOString(),
        current_transcript_count: transcripts.length
      });
      const { invoke } = await import('@tauri-apps/api/core');
      const { listen } = await import('@tauri-apps/api/event');

      // Note: stop_recording is already called by RecordingControls.stopRecordingAction
      // This function only handles post-stop processing (transcription wait, API call, navigation)
      console.log('Recording already stopped by RecordingControls, processing transcription...');

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
          const status = await invoke<{ chunks_in_queue: number, is_processing: boolean, last_activity_ms: number }>('get_transcription_status');
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
      setIsProcessingTranscript(false); // Reset processing flag
      setIsStopping(false); // Reset stopping flag

      // Wait a bit more to ensure all transcript state updates have been processed
      console.log('Waiting for transcript state updates to complete...');
      await new Promise(resolve => setTimeout(resolve, 500));

      // Save to SQLite
      // NOTE: enabled to save COMPLETE transcripts after frontend receives all updates
      // This ensures user sees all transcripts streaming in before database save
      if (isCallApi && transcriptionComplete == true) {

        setIsSavingTranscript(true);

        // Get fresh transcript state (ALL transcripts including late ones)
        const freshTranscripts = [...transcriptsRef.current];

        // Get folder_path and meeting_name from recording-stopped event
        const folderPath = sessionStorage.getItem('last_recording_folder_path');
        const savedMeetingName = sessionStorage.getItem('last_recording_meeting_name');

        console.log('💾 Saving COMPLETE transcripts to database...', {
          transcript_count: freshTranscripts.length,
          meeting_name: meetingTitle || savedMeetingName,
          folder_path: folderPath,
          sample_text: freshTranscripts.length > 0 ? freshTranscripts[0].text.substring(0, 50) + '...' : 'none',
          last_transcript: freshTranscripts.length > 0 ? freshTranscripts[freshTranscripts.length - 1].text.substring(0, 30) + '...' : 'none',
        });

        try {
          const responseData = await invoke('api_save_transcript', {
            meetingTitle: meetingTitle || savedMeetingName,
            transcripts: freshTranscripts,
            folderPath: folderPath,
          }) as any;

          const meetingId = responseData.meeting_id;
          if (!meetingId) {
            console.error('No meeting_id in response:', responseData);
            throw new Error('No meeting ID received from save operation');
          }

          console.log('✅ Successfully saved COMPLETE meeting with ID:', meetingId);
          console.log('   Transcripts:', freshTranscripts.length);
          console.log('   folder_path:', folderPath);

          // Clean up session storage
          sessionStorage.removeItem('last_recording_folder_path');
          sessionStorage.removeItem('last_recording_meeting_name');

          // Refetch meetings and set current meeting
          await refetchMeetings();

          try {
            const meetingData = await invoke('api_get_meeting', { meetingId }) as any;
            if (meetingData) {
              setCurrentMeeting({
                id: meetingId,
                title: meetingData.title
              });
              console.log('✅ Current meeting set:', meetingData.title);
            }
          } catch (error) {
            console.warn('Could not fetch meeting details, using ID only:', error);
            setCurrentMeeting({ id: meetingId, title: meetingTitle || 'New Meeting' });
          }

          // Show success toast with navigation option
          toast.success('Recording saved successfully!', {
            description: `${freshTranscripts.length} transcript segments saved.`,
            action: {
              label: 'View Meeting',
              onClick: () => {
                router.push(`/meeting-details?id=${meetingId}`);
                Analytics.trackButtonClick('view_meeting_from_toast', 'recording_complete');
              }
            },
            duration: 10000,
          });

          // Auto-navigate after a short delay
          setTimeout(() => {
            router.push(`/meeting-details?id=${meetingId}`);
            Analytics.trackPageView('meeting_details');
          }, 2000);

          setMeetings([{ id: meetingId, title: meetingTitle || savedMeetingName || 'New Meeting' }, ...meetings]);

          // Track meeting completion analytics
          try {
            // Calculate meeting duration from transcript timestamps
            let durationSeconds = 0;
            if (freshTranscripts.length > 0 && freshTranscripts[0].audio_start_time !== undefined) {
              // Use audio_end_time of last transcript if available
              const lastTranscript = freshTranscripts[freshTranscripts.length - 1];
              durationSeconds = lastTranscript.audio_end_time || lastTranscript.audio_start_time || 0;
            }

            // Calculate word count
            const transcriptWordCount = freshTranscripts
              .map(t => t.text.split(/\s+/).length)
              .reduce((a, b) => a + b, 0);

            // Calculate words per minute
            const wordsPerMinute = durationSeconds > 0 ? transcriptWordCount / (durationSeconds / 60) : 0;

            // Get meetings count today
            const meetingsToday = await Analytics.getMeetingsCountToday();

            // Track meeting completed
            await Analytics.trackMeetingCompleted(meetingId, {
              duration_seconds: durationSeconds,
              transcript_segments: freshTranscripts.length,
              transcript_word_count: transcriptWordCount,
              words_per_minute: wordsPerMinute,
              meetings_today: meetingsToday
            });

            // Update meeting count in analytics.json
            await Analytics.updateMeetingCount();

            // Check for activation (first meeting)
            const { Store } = await import('@tauri-apps/plugin-store');
            const store = await Store.load('analytics.json');
            const totalMeetings = await store.get<number>('total_meetings');

            if (totalMeetings === 1) {
              const daysSinceInstall = await Analytics.calculateDaysSince('first_launch_date');
              await Analytics.track('user_activated', {
                meetings_count: '1',
                days_since_install: daysSinceInstall?.toString() || 'null',
                first_meeting_duration_seconds: durationSeconds.toString()
              });
            }
          } catch (analyticsError) {
            console.error('Failed to track meeting completion analytics:', analyticsError);
            // Don't block user flow on analytics errors
          }

        } catch (saveError) {
          console.error('Failed to save meeting to database:', saveError);
          toast.error('Failed to save meeting', {
            description: saveError instanceof Error ? saveError.message : 'Unknown error'
          });
          throw saveError;
        } finally {
          setIsSavingTranscript(false);
        }
      }
      setIsMeetingActive(false);
      // isRecordingState already set to false at function start
      setIsRecordingDisabled(false);
    } catch (error) {
      console.error('Error in handleRecordingStop2:', error);
      // isRecordingState already set to false at function start
      setSummaryStatus('idle');
      setIsProcessingTranscript(false); // Reset on error
      setIsStopping(false); // Reset stopping flag on error
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

  const handleCopyTranscript = useCallback(() => {
    // Format timestamps as recording-relative [MM:SS] instead of wall-clock time
    const formatTime = (seconds: number | undefined): string => {
      if (seconds === undefined) return '[--:--]';
      const totalSecs = Math.floor(seconds);
      const mins = Math.floor(totalSecs / 60);
      const secs = totalSecs % 60;
      return `[${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
    };

    const fullTranscript = transcripts
      .map(t => `${formatTime(t.audio_start_time)} ${t.text}`)
      .join('\n');
    navigator.clipboard.writeText(fullTranscript);

    toast.success("Transcript copied to clipboard");
  }, [transcripts]);

  // Handle confidence indicator toggle
  const handleConfidenceToggle = (checked: boolean) => {
    setShowConfidenceIndicator(checked);
    if (typeof window !== 'undefined') {
      localStorage.setItem('showConfidenceIndicator', checked.toString());
    }
    // Trigger a custom event to notify other components
    window.dispatchEvent(new CustomEvent('confidenceIndicatorChanged', { detail: checked }));
  };

  // Listen for model download completion to auto-close modal
  useEffect(() => {
    const setupDownloadListeners = async () => {
      const unlisteners: (() => void)[] = [];

      // Listen for Whisper model download complete
      const unlistenWhisper = await listen<{ modelName: string }>('model-download-complete', (event) => {
        const { modelName } = event.payload;
        console.log('[HomePage] Whisper model download complete:', modelName);

        // Auto-close modal if the downloaded model matches the selected one
        if (transcriptModelConfig.provider === 'localWhisper' && transcriptModelConfig.model === modelName) {
          toast.success('Model ready! Closing window...', { duration: 1500 });
          setTimeout(() => setShowModelSelector(false), 1500);
        }
      });
      unlisteners.push(unlistenWhisper);

      // Listen for Parakeet model download complete
      const unlistenParakeet = await listen<{ modelName: string }>('parakeet-model-download-complete', (event) => {
        const { modelName } = event.payload;
        console.log('[HomePage] Parakeet model download complete:', modelName);

        // Auto-close modal if the downloaded model matches the selected one
        if (transcriptModelConfig.provider === 'parakeet' && transcriptModelConfig.model === modelName) {
          toast.success('Model ready! Closing window...', { duration: 1500 });
          setTimeout(() => setShowModelSelector(false), 1500);
        }
      });
      unlisteners.push(unlistenParakeet);

      return () => {
        unlisteners.forEach(unsub => unsub());
      };
    };

    setupDownloadListeners();
  }, [transcriptModelConfig]);

  const isProcessingStop = summaryStatus === 'processing' || isProcessingTranscript
  const handleRecordingStop2Ref = useRef(handleRecordingStop2);
  const handleRecordingStartRef = useRef(handleRecordingStart);
  useEffect(() => {
    handleRecordingStop2Ref.current = handleRecordingStop2;
    handleRecordingStartRef.current = handleRecordingStart;
  });

  // Expose handleRecordingStop and handleRecordingStart functions to rust using refs for stale closure issues
  useEffect(() => {
    (window as any).handleRecordingStop = (callApi: boolean = true) => {
      handleRecordingStop2Ref.current(callApi);
    };

    // Cleanup on unmount
    return () => {
      delete (window as any).handleRecordingStop;
    };
  }, []);

  useEffect(() => {
    // Honor saved model settings from backend (including OpenRouter)
    const fetchModelConfig = async () => {
      try {
        const data = await invoke('api_get_model_config') as any;
        if (data && data.provider) {
          setModelConfig(prev => ({
            ...prev,
            provider: data.provider,
            model: data.model || prev.model,
            whisperModel: data.whisperModel || prev.whisperModel,
          }));
        }
      } catch (error) {
        console.error('Failed to fetch saved model config in page.tsx:', error);
      }
    };
    fetchModelConfig();
  }, []);

  // Load device preferences on startup
  useEffect(() => {
    const loadDevicePreferences = async () => {
      try {
        const prefs = await invoke('get_recording_preferences') as any;
        if (prefs && (prefs.preferred_mic_device || prefs.preferred_system_device)) {
          setSelectedDevices({
            micDevice: prefs.preferred_mic_device,
            systemDevice: prefs.preferred_system_device
          });
          console.log('Loaded device preferences:', prefs);
        }
      } catch (error) {
        console.log('No device preferences found or failed to load:', error);
      }
    };
    loadDevicePreferences();
  }, []);

  // Load language preference on startup
  useEffect(() => {
    const loadLanguagePreference = async () => {
      try {
        const language = await invoke('get_language_preference') as string;
        if (language) {
          setSelectedLanguage(language);
          console.log('Loaded language preference:', language);
        }
      } catch (error) {
        console.log('No language preference found or failed to load, using default (auto-translate):', error);
        // Default to 'auto-translate' (Auto Detect with English translation) if no preference is saved
        setSelectedLanguage('auto-translate');
      }
    };
    loadLanguagePreference();
  }, []);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen bg-gray-50"
    >
      {/* SettingsModal starts here; Remove the content*/}
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
      {/* SettingsModal ends here */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left side - Transcript */}
        <div ref={transcriptContainerRef} className="w-full border-r border-gray-200 bg-white flex flex-col overflow-y-auto">
          {/* Title area - Sticky header */}
          <div className="sticky top-0 z-10 bg-white p-4 border-gray-200">
            <div className="flex flex-col space-y-3">
              <div className="flex  flex-col space-y-2">
                <div className="flex justify-center  items-center space-x-2">
                  <ButtonGroup>
                    {transcripts?.length > 0 && (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          handleCopyTranscript();
                        }}
                        title="Copy Transcript"
                      >
                        <Copy />
                        <span className='hidden md:inline'>
                          Copy
                        </span>
                      </Button>
                    )}
                    {transcriptModelConfig.provider === "localWhisper" &&
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setShowLanguageSettings(true)}
                        title="Language"
                      >
                        <GlobeIcon />
                        <span className='hidden md:inline'>
                          Language
                        </span>
                      </Button>
                    }
                  </ButtonGroup>
                </div>
              </div>
            </div>
          </div>

          {/* Permission Warning */}
          {!isRecording && !isCheckingPermissions && (
            <div className="flex justify-center px-4 pt-4">
              <PermissionWarning
                hasMicrophone={hasMicrophone}
                hasSystemAudio={hasSystemAudio}
                onRecheck={checkPermissions}
                isRechecking={isCheckingPermissions}
              />
            </div>
          )}

          {/* Transcript content */}
          <div className="pb-20">
            <div className="flex justify-center">
              <div className="w-2/3 max-w-[750px]">
                <TranscriptView
                  transcripts={transcripts}
                  isRecording={recordingState.isRecording}
                  isPaused={recordingState.isPaused}
                  isProcessing={isProcessingStop}
                  isStopping={isStopping}
                  enableStreaming={recordingState.isRecording}
                />
              </div>
            </div>
          </div>

          {/* Recording controls - only show when permissions are granted or already recording and not showing status messages */}
          {(hasMicrophone || isRecording) && !isProcessingStop && !isSavingTranscript && (
            <div className="fixed bottom-12 left-0 right-0 z-10">
              <div
                className="flex justify-center pl-8 transition-[margin] duration-300"
                style={{
                  marginLeft: sidebarCollapsed ? '4rem' : '16rem'
                }}
              >
                <div className="w-2/3 max-w-[750px] flex justify-center">
                  <div className="bg-white rounded-full shadow-lg flex items-center">
                    <RecordingControls
                      isRecording={recordingState.isRecording}
                      onRecordingStop={(callApi = true) => handleRecordingStop2(callApi)}
                      onRecordingStart={handleRecordingStart}
                      onTranscriptReceived={handleTranscriptUpdate}
                      onStopInitiated={() => setIsStopping(true)}
                      barHeights={barHeights}
                      onTranscriptionError={(message) => {
                        setErrorMessage(message);
                        setShowErrorAlert(true);
                      }}
                      isRecordingDisabled={isRecordingDisabled}
                      isParentProcessing={isProcessingStop}
                      selectedDevices={selectedDevices}
                      meetingName={meetingTitle}
                    />
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Processing status overlay */}
          {summaryStatus === 'processing' && !isRecording && (
            <div className="fixed bottom-4 left-0 right-0 z-10">
              <div
                className="flex justify-center pl-8 transition-[margin] duration-300"
                style={{
                  marginLeft: sidebarCollapsed ? '4rem' : '16rem'
                }}
              >
                <div className="w-2/3 max-w-[750px] flex justify-center">
                  <div className="bg-white rounded-lg shadow-lg px-4 py-2 flex items-center space-x-2">
                    <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-gray-900"></div>
                    <span className="text-sm text-gray-700">Finalizing transcription...</span>
                  </div>
                </div>
              </div>
            </div>
          )}
          {isSavingTranscript && (
            <div className="fixed bottom-4 left-0 right-0 z-10">
              <div
                className="flex justify-center pl-8 transition-[margin] duration-300"
                style={{
                  marginLeft: sidebarCollapsed ? '4rem' : '16rem'
                }}
              >
                <div className="w-2/3 max-w-[750px] flex justify-center">
                  <div className="bg-white rounded-lg shadow-lg px-4 py-2 flex items-center space-x-2">
                    <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-gray-900"></div>
                    <span className="text-sm text-gray-700">Saving transcript...</span>
                  </div>
                </div>
              </div>
            </div>
          )}
          {/* SettingModal is enough you can remove from here */}
          {/* Preferences Modal (Settings) */}
          {showModelSettings && (
            <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4">
              <div className="bg-white rounded-lg shadow-xl max-w-4xl w-full max-h-[90vh] overflow-hidden flex flex-col">
                {/* Header */}
                <div className="flex justify-between items-center p-6 border-b">
                  <h3 className="text-xl font-semibold text-gray-900">Preferences</h3>
                  <button
                    onClick={() => setShowModelSettings(false)}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                {/* Content - Scrollable */}
                <div className="flex-1 overflow-y-auto p-6 space-y-8">
                  {/* General Preferences Section */}
                  <PreferenceSettings />

                  {/* Divider */}
                  <div className="border-t pt-8">
                    <h4 className="text-lg font-semibold text-gray-900 mb-4">AI Model Configuration</h4>
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
                            <option value="openrouter">OpenRouter</option>
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
                                className={`bg-white p-4 rounded-lg shadow cursor-pointer transition-colors ${modelConfig.model === model.name ? 'ring-2 ring-blue-500 bg-blue-50' : 'hover:bg-gray-50'
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
                  </div>
                </div>

                {/* Footer */}
                <div className="border-t p-6 flex justify-end">
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

          {/* Device Settings Modal */}
          {showDeviceSettings && (
            <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
              <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4 shadow-xl">
                <div className="flex justify-between items-center mb-4">
                  <h3 className="text-lg font-semibold text-gray-900">Audio Device Settings</h3>
                  <button
                    onClick={() => setShowDeviceSettings(false)}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                <DeviceSelection
                  selectedDevices={selectedDevices}
                  onDeviceChange={setSelectedDevices}
                  disabled={isRecording}
                />

                <div className="mt-6 flex justify-end">
                  <button
                    onClick={() => {
                      const micDevice = selectedDevices.micDevice || 'Default';
                      const systemDevice = selectedDevices.systemDevice || 'Default';
                      toast.success("Devices selected", {
                        description: `Microphone: ${micDevice}, System Audio: ${systemDevice}`
                      });
                      setShowDeviceSettings(false);
                    }}
                    className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
                  >
                    Done
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Language Settings Modal */}
          {showLanguageSettings && (
            <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
              <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4 shadow-xl">
                <div className="flex justify-between items-center mb-4">
                  <h3 className="text-lg font-semibold text-gray-900">Language Settings</h3>
                  <button
                    onClick={() => setShowLanguageSettings(false)}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                <LanguageSelection
                  selectedLanguage={selectedLanguage}
                  onLanguageChange={setSelectedLanguage}
                  disabled={isRecording}
                  provider={transcriptModelConfig.provider}
                />

                <div className="mt-6 flex justify-end">
                  <button
                    onClick={() => setShowLanguageSettings(false)}
                    className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
                  >
                    Done
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Model Selection Modal - shown when model loading fails */}
          {showModelSelector && (
            <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
              <div className="bg-white rounded-lg max-w-4xl w-full mx-4 shadow-xl max-h-[90vh] flex flex-col">
                {/* Fixed Header */}
                <div className="flex justify-between items-center p-6 pb-4 border-b border-gray-200">
                  <h3 className="text-lg font-semibold text-gray-900">
                    {modelSelectorMessage ? 'Speech Recognition Setup Required' : 'Transcription Model Settings'}
                  </h3>
                  <button
                    onClick={() => {
                      setShowModelSelector(false);
                      setModelSelectorMessage(''); // Clear the message when closing
                    }}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                {/* Scrollable Content */}
                <div className="flex-1 overflow-y-auto p-6 pt-4">
                  <TranscriptSettings
                    transcriptModelConfig={transcriptModelConfig}
                    setTranscriptModelConfig={setTranscriptModelConfig}
                    onModelSelect={() => {
                      setShowModelSelector(false);
                      setModelSelectorMessage('');
                    }}
                  />
                </div>

                {/* Fixed Footer */}
                <div className="p-6 pt-4 border-t border-gray-200 flex items-center justify-between">
                  {/* Left side: Confidence Indicator Toggle */}
                  <div className="flex items-center gap-3">
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input
                        type="checkbox"
                        checked={showConfidenceIndicator}
                        onChange={(e) => handleConfidenceToggle(e.target.checked)}
                        className="sr-only peer"
                      />
                      <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                    </label>
                    <div>
                      <p className="text-sm font-medium text-gray-700">Show Confidence Indicators</p>
                      <p className="text-xs text-gray-500">Display colored dots showing transcription confidence quality</p>
                    </div>
                  </div>

                  {/* Right side: Done Button */}
                  <button
                    onClick={() => {
                      setShowModelSelector(false);
                      setModelSelectorMessage(''); // Clear the message when closing
                    }}
                    className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 rounded-md hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-gray-500"
                  >
                    {modelSelectorMessage ? 'Cancel' : 'Done'}
                  </button>
                </div>
              </div>
            </div>
          )}
          {/* SettingModal is enough you can remove to here */}
        </div>
      </div>
    </motion.div>
  );
}
