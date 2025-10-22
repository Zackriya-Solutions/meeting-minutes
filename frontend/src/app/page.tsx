'use client';

import { useState, useEffect, useContext, useCallback, useRef } from 'react';
import { motion } from 'framer-motion';
import { Transcript, TranscriptUpdate, Summary, SummaryResponse } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { TranscriptView } from '@/components/TranscriptView';
import { RecordingControls } from '@/components/RecordingControls';
import { AISummary } from '@/components/AISummary';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { TranscriptSettings, TranscriptModelProps } from '@/components/TranscriptSettings';
import { LanguageSelection } from '@/components/LanguageSelection';
import { PermissionWarning } from '@/components/PermissionWarning';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState } from '@/contexts/RecordingStateContext';
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
import { showRecordingNotification } from '@/lib/recordingNotification';
import { Button } from '@/components/ui/button';
import { Copy, GlobeIcon, Settings } from 'lucide-react';
import { MicrophoneIcon } from '@heroicons/react/24/outline';
import { toast } from 'sonner';
import { ButtonGroup } from '@/components/ui/button-group';



interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openrouter';
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
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
    apiKey: null
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

  // Compute effective recording state for UI: override with local state during stop transition
  // This ensures immediate UI feedback when stop is pressed, while preserving backend-synced state for reload functionality
  const effectiveIsRecording = isProcessingTranscript ? false : recordingState.isRecording;

  const { setCurrentMeeting, setMeetings, meetings, isMeetingActive, setIsMeetingActive, setIsRecording: setSidebarIsRecording, serverAddress, isCollapsed: sidebarCollapsed, refetchMeetings } = useSidebar();
  const handleNavigation = useNavigation('', ''); // Initialize with empty values
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
      const { appDataDir } = await import('@tauri-apps/api/path');
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
      // Show summary button if we have transcript content
      if (transcripts.length > 0) {
        setShowSummary(true);
      } else {
        console.log('No transcript content available');
      }
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

  // Handle transcript configuration save
  const handleSaveTranscriptConfig = async (config: TranscriptModelProps) => {
    try {
      console.log('[HomePage] Saving transcript config:', config);
      await invoke('api_save_transcript_config', {
        provider: config.provider,
        model: config.model,
        apiKey: config.apiKey
      });
      console.log('[HomePage] ✅ Successfully saved transcript config');
    } catch (error) {
      console.error('[HomePage] ❌ Failed to save transcript config:', error);
    }
  };

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

  const isSummaryLoading = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

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
                  {/* {!isRecording && transcripts?.length === 0 && ( */}
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setShowModelSelector(true)}
                      title="Transcription Model Settings"
                    >
                      <Settings />
                      <span className='hidden md:inline'>
                        Model
                      </span>
                    </Button>
                  
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setShowDeviceSettings(true)}
                    title="Input/Output devices selection"
                  >
                    <MicrophoneIcon />
                    <span className='hidden md:inline'>
                      Devices
                    </span>
                  </Button>
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
                  </ButtonGroup>
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
                  enableStreaming={recordingState.isRecording }
                />
              </div>
            </div>
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
                  {/* Only show warning if there's an error message (triggered by transcription error) */}
                  {modelSelectorMessage && (
                    <div className="mb-6 p-4 bg-yellow-50 border border-yellow-200 rounded-lg">
                      <div className="flex items-start space-x-3">
                        <span className="text-yellow-600 text-xl">⚠️</span>
                        <div>
                          <h4 className="font-medium text-yellow-800 mb-1">Model Required</h4>
                          <p className="text-sm text-yellow-700">
                            {modelSelectorMessage}
                          </p>
                        </div>
                      </div>
                    </div>
                  )}

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
        </div>

        {/* Right side - AI Summary */}
        {/* <div className="flex-1 overflow-y-auto bg-white"> */}
        {/*   <div className="p-4 border-b border-gray-200"> */}
        {/*     <div className="flex items-center"> */}
        {/*       <EditableTitle */}
        {/*         title={meetingTitle} */}
        {/*         isEditing={isEditingTitle} */}
        {/*         onStartEditing={() => setIsEditingTitle(true)} */}
        {/*         onFinishEditing={() => setIsEditingTitle(false)} */}
        {/*         onChange={handleTitleChange} */}
        {/*       /> */}
        {/*     </div> */}
        {/*   </div> */}
        {/*   {/* {isSummaryLoading ? ( */}
        {/*     <div className="flex items-center justify-center h-full"> */}
        {/*       <div className="text-center"> */}
        {/*         <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mb-4"></div> */}
        {/*         <p className="text-gray-600">Generating AI Summary...</p> */}
        {/*       </div> */}
        {/*     </div> */}
        {/*   ) : showSummary && ( */}
        {/*     <div className="max-w-4xl mx-auto p-6"> */}
        {/*       {summaryResponse && ( */}
        {/*         <div className="fixed bottom-0 left-0 right-0 bg-white shadow-lg p-4 max-h-1/3 overflow-y-auto"> */}
        {/*           <h3 className="text-lg font-semibold mb-2">Meeting Summary</h3> */}
        {/*           <div className="grid grid-cols-2 gap-4"> */}
        {/*             <div className="bg-white p-4 rounded-lg shadow-sm"> */}
        {/*               <h4 className="font-medium mb-1">Key Points</h4> */}
        {/*               <ul className="list-disc pl-4"> */}
        {/*                 {summaryResponse.summary.key_points.blocks.map((block, i) => ( */}
        {/*                   <li key={i} className="text-sm">{block.content}</li> */}
        {/*                 ))} */}
        {/*               </ul> */}
        {/*             </div> */}
        {/*             <div className="bg-white p-4 rounded-lg shadow-sm mt-4"> */}
        {/*               <h4 className="font-medium mb-1">Action Items</h4> */}
        {/*               <ul className="list-disc pl-4"> */}
        {/*                 {summaryResponse.summary.action_items.blocks.map((block, i) => ( */}
        {/*                   <li key={i} className="text-sm">{block.content}</li> */}
        {/*                 ))} */}
        {/*               </ul> */}
        {/*             </div> */}
        {/*             <div className="bg-white p-4 rounded-lg shadow-sm mt-4"> */}
        {/*               <h4 className="font-medium mb-1">Decisions</h4> */}
        {/*               <ul className="list-disc pl-4"> */}
        {/*                 {summaryResponse.summary.decisions.blocks.map((block, i) => ( */}
        {/*                   <li key={i} className="text-sm">{block.content}</li> */}
        {/*                 ))} */}
        {/*               </ul> */}
        {/*             </div> */}
        {/*             <div className="bg-white p-4 rounded-lg shadow-sm mt-4"> */}
        {/*               <h4 className="font-medium mb-1">Main Topics</h4> */}
        {/*               <ul className="list-disc pl-4"> */}
        {/*                 {summaryResponse.summary.main_topics.blocks.map((block, i) => ( */}
        {/*                   <li key={i} className="text-sm">{block.content}</li> */}
        {/*                 ))} */}
        {/*               </ul> */}
        {/*             </div> */}
        {/*           </div> */}
        {/*           {summaryResponse.raw_summary ? ( */}
        {/*             <div className="mt-4"> */}
        {/*               <h4 className="font-medium mb-1">Full Summary</h4> */}
        {/*               <p className="text-sm whitespace-pre-wrap">{summaryResponse.raw_summary}</p> */}
        {/*             </div> */}
        {/*           ) : null} */}
        {/*         </div> */}
        {/*       )} */}
        {/*       <div className="flex-1 overflow-y-auto p-4"> */}
        {/*         <AISummary  */}
        {/*           summary={aiSummary}  */}
        {/*           status={summaryStatus}  */}
        {/*           error={summaryError} */}
        {/*           onSummaryChange={(newSummary) => setAiSummary(newSummary)} */}
        {/*           onRegenerateSummary={handleRegenerateSummary} */}
        {/*         /> */}
        {/*       </div> */}
        {/*       {summaryStatus !== 'idle' && ( */}
        {/*         <div className={`mt-4 p-4 rounded-lg ${ */}
        {/*           summaryStatus === 'error' ? 'bg-red-100 text-red-700' : */}
        {/*           summaryStatus === 'completed' ? 'bg-green-100 text-green-700' : */}
        {/*           'bg-blue-100 text-blue-700' */}
        {/*         }`}> */}
        {/*           <p className="text-sm font-medium">{getSummaryStatusMessage(summaryStatus)}</p> */}
        {/*         </div> */}
        {/*       )} */}
        {/*     </div> */}
        {/*   )} */}        {/* </div> */}
      </div>
    </motion.div>
  );
}
