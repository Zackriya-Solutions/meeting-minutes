'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { motion } from 'framer-motion';
import { TranscriptUpdate } from '@/types';
import { RecordingControls } from '@/components/RecordingControls';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { StatusOverlays } from '@/app/_components/StatusOverlays';
import { listen } from '@tauri-apps/api/event';
import { useRouter } from 'next/navigation';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';
import { SettingsModals } from './_components/SettingsModal';
import { TranscriptPanel } from './_components/TranscriptPanel';
import { storageService } from '@/services/storageService';
import { recordingService } from '@/services/recordingService';
import { transcriptService } from '@/services/transcriptService';



type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

export default function Home() {
  // Use contexts for state management
  const {
    transcripts,
    transcriptsRef,
    addTranscript,
    copyTranscript,
    flushBuffer,
    transcriptContainerRef,
    meetingTitle,
    setMeetingTitle,
    clearTranscripts
  } = useTranscripts();

  const {
    modelConfig,
    setModelConfig,
    transcriptModelConfig,
    setTranscriptModelConfig,
    selectedDevices,
    setSelectedDevices,
    selectedLanguage,
    setSelectedLanguage,
    showConfidenceIndicator,
    toggleConfidenceIndicator,
    models,
    modelOptions,
    error
  } = useConfig();

  // Local page state (not moved to contexts)
  const [isRecording, setIsRecordingState] = useState(false);
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [barHeights, setBarHeights] = useState(['58%', '76%', '58%']);
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [showErrorAlert, setShowErrorAlert] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');
  const [showChunkDropWarning, setShowChunkDropWarning] = useState(false);
  const [chunkDropMessage, setChunkDropMessage] = useState('');
  const [isSavingTranscript, setIsSavingTranscript] = useState(false);
  const [isRecordingDisabled, setIsRecordingDisabled] = useState(false);
  const [showDeviceSettings, setShowDeviceSettings] = useState(false);
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [modelSelectorMessage, setModelSelectorMessage] = useState('');
  const [showLanguageSettings, setShowLanguageSettings] = useState(false);
  const [isProcessingTranscript, setIsProcessingTranscript] = useState(false);
  const [isStopping, setIsStopping] = useState(false);

  // Permission check hook
  const { hasMicrophone, hasSystemAudio, isChecking: isCheckingPermissions, checkPermissions } = usePermissionCheck();

  // Recording state context - provides backend-synced state
  const recordingState = useRecordingState();

  const { setCurrentMeeting, setMeetings, meetings, isMeetingActive, setIsMeetingActive, setIsRecording: setSidebarIsRecording, isCollapsed: sidebarCollapsed, refetchMeetings } = useSidebar();
  const router = useRouter();

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  useEffect(() => {
    setCurrentMeeting({ id: 'intro-call', title: meetingTitle });

  }, [meetingTitle, setCurrentMeeting]);

  useEffect(() => {
    console.log('Setting up recording state check effect, current isRecording:', isRecording);

    const checkRecordingState = async () => {
      try {
        console.log('checkRecordingState called');
        console.log('About to call is_recording command');
        const isCurrentlyRecording = await recordingService.isRecording();
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
      clearTranscripts(); // Clear previous transcripts when starting new recording
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
            const result = await recordingService.startRecordingWithDevices(
              selectedDevices?.micDevice || null,
              selectedDevices?.systemDevice || null,
              generatedMeetingTitle
            );
            console.log('Auto-start backend recording result:', result);

            // Update UI state after successful backend start
            setMeetingTitle(generatedMeetingTitle);
            setIsRecordingState(true);
            clearTranscripts();
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
          const status = await transcriptService.getTranscriptionStatus();
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
      flushBuffer();
      const flushEndTime = Date.now();
      console.log('✅ Final buffer flush completed', {
        flush_duration: flushEndTime - flushStartTime,
        total_time_since_stop: flushEndTime - stopStartTime,
        final_transcript_count: transcripts.length
      });

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
          const responseData = await storageService.saveMeeting(
            meetingTitle || savedMeetingName || 'New Meeting',
            freshTranscripts,
            folderPath
          );

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
            const meetingData = await storageService.getMeeting(meetingId);
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
            clearTranscripts()
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

  // handleTranscriptUpdate - delegate to context
  const handleTranscriptUpdate = useCallback((update: TranscriptUpdate) => {
    addTranscript(update);
  }, [addTranscript]);

  // handleCopyTranscript - delegate to context
  const handleCopyTranscript = useCallback(() => {
    copyTranscript();
  }, [copyTranscript]);

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

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen bg-gray-50"
    >
      {/* All Modals supported*/}
      <SettingsModals
        modals={{
          modelSettings: showModelSettings,
          deviceSettings: showDeviceSettings,
          languageSettings: showLanguageSettings,
          modelSelector: showModelSelector,
          errorAlert: showErrorAlert,
          chunkDropWarning: showChunkDropWarning,
        }}
        messages={{
          errorAlert: errorMessage,
          chunkDropWarning: chunkDropMessage,
          modelSelector: modelSelectorMessage,
        }}
        onClose={(name) => {
          if (name === 'modelSettings') setShowModelSettings(false);
          if (name === 'deviceSettings') setShowDeviceSettings(false);
          if (name === 'languageSettings') setShowLanguageSettings(false);
          if (name === 'modelSelector') {
            setShowModelSelector(false);
            setModelSelectorMessage('');
          }
          if (name === 'errorAlert') setShowErrorAlert(false);
          if (name === 'chunkDropWarning') setShowChunkDropWarning(false);
        }}
        modelConfig={modelConfig}
        setModelConfig={setModelConfig}
        models={models}
        error={error}
        selectedDevices={selectedDevices}
        setSelectedDevices={setSelectedDevices}
        isRecording={isRecording}
        selectedLanguage={selectedLanguage}
        setSelectedLanguage={setSelectedLanguage}
        transcriptModelConfig={transcriptModelConfig}
        setTranscriptModelConfig={setTranscriptModelConfig}
        showConfidenceIndicator={showConfidenceIndicator}
        handleConfidenceToggle={toggleConfidenceIndicator}
      />
      <div className="flex flex-1 overflow-hidden">
        <TranscriptPanel
          transcripts={transcripts}
          isRecording={recordingState.isRecording}
          isPaused={recordingState.isPaused}
          isProcessingStop={isProcessingStop}
          isStopping={isStopping}
          hasMicrophone={hasMicrophone}
          hasSystemAudio={hasSystemAudio}
          isCheckingPermissions={isCheckingPermissions}
          onCheckPermissions={checkPermissions}
          transcriptModelConfig={transcriptModelConfig}
          onCopyTranscript={handleCopyTranscript}
          onOpenLanguageSettings={() => setShowLanguageSettings(true)}
          containerRef={transcriptContainerRef}
        />

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
                    onTranscriptReceived={() => { }} // Not actually used by RecordingControls
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

        {/* Status Overlays - Processing and Saving */}
        <StatusOverlays
          isProcessing={summaryStatus === 'processing' && !isRecording}
          isSaving={isSavingTranscript}
          sidebarCollapsed={sidebarCollapsed}
        />
      </div>
    </motion.div>
  );
}
