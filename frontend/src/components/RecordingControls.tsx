'use client';

import { invoke } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { useCallback, useEffect, useState, useRef } from 'react';
import { Play, Pause, Square, Mic } from 'lucide-react';
import { ProcessRequest, SummaryResponse } from '@/types/summary';
import { listen } from '@tauri-apps/api/event';
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import Analytics from '@/lib/analytics';

interface RecordingControlsProps {
  isRecording: boolean;
  barHeights: string[];
  onRecordingStop: (callApi?: boolean) => void;
  onRecordingStart: () => void;
  onTranscriptReceived: (summary: SummaryResponse) => void;
  onTranscriptionError?: (message: string) => void;
  isRecordingDisabled: boolean;
  isParentProcessing: boolean;
  selectedDevices?: {
    micDevice: string | null;
    systemDevice: string | null;
  };
  meetingName?: string;
}

export const RecordingControls: React.FC<RecordingControlsProps> = ({
  isRecording,
  barHeights,
  onRecordingStop,
  onRecordingStart,
  onTranscriptReceived,
  onTranscriptionError,
  isRecordingDisabled,
  isParentProcessing,
  selectedDevices,
  meetingName,
}) => {
  const [showPlayback, setShowPlayback] = useState(false);
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string>('');
  const [isProcessing, setIsProcessing] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [isPausing, setIsPausing] = useState(false);
  const [isResuming, setIsResuming] = useState(false);
  const MIN_RECORDING_DURATION = 2000; // 2 seconds minimum recording time
  const [transcriptionErrors, setTranscriptionErrors] = useState(0);


  const currentTime = 0;
  const duration = 0;
  const isPlaying = false;
  const progress = 0;

  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  };

  useEffect(() => {
    const checkTauri = async () => {
      try {
        const result = await invoke('is_recording');
        console.log('Tauri is initialized and ready, is_recording result:', result);
      } catch (error) {
        console.error('Tauri initialization error:', error);
        alert('Failed to initialize recording. Please check the console for details.');
      }
    };
    checkTauri();
  }, []);

  const handleStartRecording = useCallback(async () => {
    if (isStarting) return;
    console.log('Starting recording...');
    console.log('Selected devices:', selectedDevices);
    console.log('Meeting name:', meetingName);
    console.log('Current isRecording state:', isRecording);
    setIsStarting(true);
    setShowPlayback(false);
    setTranscript(''); // Clear any previous transcript
    
    try {
      // Generate meeting title here to ensure it's available for the backend call
      const now = new Date();
      const day = String(now.getDate()).padStart(2, '0');
      const month = String(now.getMonth() + 1).padStart(2, '0');
      const year = String(now.getFullYear()).slice(-2);
      const hours = String(now.getHours()).padStart(2, '0');
      const minutes = String(now.getMinutes()).padStart(2, '0');
      const seconds = String(now.getSeconds()).padStart(2, '0');
      const generatedMeetingTitle = `Meeting_${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
      
      // Use the correct command with device parameters
      if (selectedDevices || meetingName || generatedMeetingTitle) {
        console.log('Using start_recording_with_devices_and_meeting with:', {
          mic_device_name: selectedDevices?.micDevice || null,
          system_device_name: selectedDevices?.systemDevice || null,
          meeting_name: meetingName || generatedMeetingTitle
        });
        const result = await invoke('start_recording_with_devices_and_meeting', {
          mic_device_name: selectedDevices?.micDevice || null,
          system_device_name: selectedDevices?.systemDevice || null,
          meeting_name: meetingName || generatedMeetingTitle
        });
        console.log('Backend recording start result:', result);
      } else {
        console.log('Using start_recording (no devices/meeting specified)');
        const result = await invoke('start_recording');
        console.log('Backend recording start result:', result);
      }
      console.log('Recording started successfully');
      setIsProcessing(false);
      
      // Call onRecordingStart after successful recording start
      onRecordingStart();
    } catch (error) {
      console.error('Failed to start recording:', error);
      console.error('Error details:', {
        message: error instanceof Error ? error.message : String(error),
        name: error instanceof Error ? error.name : 'Unknown',
        stack: error instanceof Error ? error.stack : undefined
      });
      alert('Failed to start recording. Please check the console for details.');
    } finally {
      setIsStarting(false);
    }
  }, [onRecordingStart, isStarting, selectedDevices, meetingName]);

  const stopRecordingAction = useCallback(async () => {
    console.log('Executing stop recording...');
    try {
      setIsProcessing(true);
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const savePath = `${dataDir}/recording-${timestamp}.wav`;
      
      console.log('Saving recording to:', savePath);
      console.log('About to call stop_recording command');
      const result = await invoke('stop_recording', { 
        args: {
          save_path: savePath
        }
      });
      console.log('stop_recording command completed successfully:', result);
      
      setRecordingPath(savePath);
      // setShowPlayback(true);
      setIsProcessing(false);
      
      // Track successful transcription
      Analytics.trackTranscriptionSuccess();
      
      onRecordingStop(true);
    } catch (error) {
      console.error('Failed to stop recording:', error);
      if (error instanceof Error) {
        console.error('Error details:', {
          message: error.message,
          name: error.name,
          stack: error.stack,
        });
        if (error.message.includes('No recording in progress')) {
          return;
        }
      } else if (typeof error === 'string' && error.includes('No recording in progress')) {
        return;
      } else if (error && typeof error === 'object' && 'toString' in error) {
        if (error.toString().includes('No recording in progress')) {
          return;
        }
      }
      setIsProcessing(false);
      onRecordingStop(false);
    } finally {
      setIsStopping(false);
    }
  }, [onRecordingStop]);

  const handleStopRecording = useCallback(async () => {
    console.log('handleStopRecording called - isRecording:', isRecording, 'isStarting:', isStarting, 'isStopping:', isStopping);
    if (!isRecording || isStarting || isStopping) {
      console.log('Early return from handleStopRecording due to state check');
      return;
    }

    console.log('Stopping recording...');
    setIsStopping(true);

    // Immediately trigger the stop action
    await stopRecordingAction();
  }, [isRecording, isStarting, isStopping, stopRecordingAction]);

  const handlePauseRecording = useCallback(async () => {
    if (!isRecording || isPaused || isPausing) return;

    console.log('Pausing recording...');
    setIsPausing(true);

    try {
      await invoke('pause_recording');
      setIsPaused(true);
      console.log('Recording paused successfully');
    } catch (error) {
      console.error('Failed to pause recording:', error);
      alert('Failed to pause recording. Please check the console for details.');
    } finally {
      setIsPausing(false);
    }
  }, [isRecording, isPaused, isPausing]);

  const handleResumeRecording = useCallback(async () => {
    if (!isRecording || !isPaused || isResuming) return;

    console.log('Resuming recording...');
    setIsResuming(true);

    try {
      await invoke('resume_recording');
      setIsPaused(false);
      console.log('Recording resumed successfully');
    } catch (error) {
      console.error('Failed to resume recording:', error);
      alert('Failed to resume recording. Please check the console for details.');
    } finally {
      setIsResuming(false);
    }
  }, [isRecording, isPaused, isResuming]);

  useEffect(() => {
    return () => {
      // Cleanup on unmount if needed
    };
  }, []);

  useEffect(() => {
    console.log('Setting up recording event listeners');
    let unsubscribes: (() => void)[] = [];

    const setupListeners = async () => {
      try {
        // Transcript error listener
        const transcriptErrorUnsubscribe = await listen('transcript-error', (event) => {
          console.log('transcript-error event received:', event);
          console.error('Transcription error received:', event.payload);
          const errorMessage = event.payload as string;

          Analytics.trackTranscriptionError(errorMessage);
          console.log('Tracked transcription error:', errorMessage);

          setTranscriptionErrors(prev => {
            const newCount = prev + 1;
            console.log('Transcription error count incremented:', newCount);
            return newCount;
          });
          setIsProcessing(false);
          console.log('Calling onRecordingStop(false) due to transcript error');
          onRecordingStop(false);
          if (onTranscriptionError) {
            onTranscriptionError(errorMessage);
          }
        });

        // Recording paused listener
        const pausedUnsubscribe = await listen('recording-paused', (event) => {
          console.log('recording-paused event received:', event);
          setIsPaused(true);
        });

        // Recording resumed listener
        const resumedUnsubscribe = await listen('recording-resumed', (event) => {
          console.log('recording-resumed event received:', event);
          setIsPaused(false);
        });

        unsubscribes = [transcriptErrorUnsubscribe, pausedUnsubscribe, resumedUnsubscribe];
        console.log('Recording event listeners set up successfully');
      } catch (error) {
        console.error('Failed to set up recording event listeners:', error);
      }
    };

    setupListeners();

    return () => {
      console.log('Cleaning up recording event listeners');
      unsubscribes.forEach(unsubscribe => {
        if (unsubscribe && typeof unsubscribe === 'function') {
          unsubscribe();
        }
      });
    };
  }, [onRecordingStop, onTranscriptionError]);

    return (
    <div className="flex flex-col space-y-2">
      <div className="flex items-center space-x-2 bg-white rounded-full shadow-lg px-4 py-2">
        {isProcessing && !isParentProcessing ? (
          <div className="flex items-center space-x-2">
            <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-gray-900"></div>
            <span className="text-sm text-gray-600">Processing recording...</span>
          </div>
        ) : (
          <>
            {showPlayback ? (
              <>
                <button
                  onClick={handleStartRecording}
                  className="w-10 h-10 flex items-center justify-center bg-red-500 rounded-full text-white hover:bg-red-600 transition-colors"
                >
                  <Mic size={16} />
                </button>

                <div className="w-px h-6 bg-gray-200 mx-1" />

                <div className="flex items-center space-x-1 mx-2">
                  <div className="text-sm text-gray-600 min-w-[40px]">
                    {formatTime(currentTime)}
                  </div>
                  <div 
                    className="relative w-24 h-1 bg-gray-200 rounded-full"
                  >
                    <div 
                      className="absolute h-full bg-blue-500 rounded-full" 
                      style={{ width: `${progress}%` }}
                    />
                  </div>
                  <div className="text-sm text-gray-600 min-w-[40px]">
                    {formatTime(duration)}
                  </div>
                </div>

                <button 
                  className="w-10 h-10 flex items-center justify-center bg-gray-300 rounded-full text-white cursor-not-allowed"
                  disabled
                >
                  <Play size={16} />
                </button>
              </>
            ) : (
              <>
                {!isRecording ? (
                  // Start recording button
                  <button
                    onClick={() => {
                      Analytics.trackButtonClick('start_recording', 'recording_controls');
                      handleStartRecording();
                    }}
                    disabled={isStarting || isProcessing || isRecordingDisabled}
                    className={`w-12 h-12 flex items-center justify-center ${
                      isStarting || isProcessing ? 'bg-gray-400' : 'bg-red-500 hover:bg-red-600'
                    } rounded-full text-white transition-colors relative`}
                  >
                    <Mic size={20} />
                  </button>
                ) : (
                  // Recording controls (pause/resume + stop)
                  <>
                    <button
                      onClick={() => {
                        if (isPaused) {
                          Analytics.trackButtonClick('resume_recording', 'recording_controls');
                          handleResumeRecording();
                        } else {
                          Analytics.trackButtonClick('pause_recording', 'recording_controls');
                          handlePauseRecording();
                        }
                      }}
                      disabled={isPausing || isResuming || isStopping}
                      className={`w-10 h-10 flex items-center justify-center ${
                        isPausing || isResuming || isStopping ? 'bg-gray-400' : 'bg-blue-500 hover:bg-blue-600'
                      } rounded-full text-white transition-colors relative`}
                    >
                      {isPaused ? <Play size={16} /> : <Pause size={16} />}
                      {(isPausing || isResuming) && (
                        <div className="absolute -top-8 text-gray-600 font-medium text-xs">
                          {isPausing ? 'Pausing...' : 'Resuming...'}
                        </div>
                      )}
                    </button>

                    <button
                      onClick={() => {
                        Analytics.trackButtonClick('stop_recording', 'recording_controls');
                        handleStopRecording();
                      }}
                      disabled={isStopping || isPausing || isResuming}
                      className={`w-10 h-10 flex items-center justify-center ${
                        isStopping || isPausing || isResuming ? 'bg-gray-400' : 'bg-red-500 hover:bg-red-600'
                      } rounded-full text-white transition-colors relative`}
                    >
                      <Square size={16} />
                      {isStopping && (
                        <div className="absolute -top-8 text-gray-600 font-medium text-xs">
                          Stopping...
                        </div>
                      )}
                    </button>
                  </>
                )}

                <div className="flex items-center space-x-1 mx-4">
                  {barHeights.map((height, index) => (
                    <div
                      key={index}
                      className={`w-1 rounded-full transition-all duration-200 ${
                        isPaused ? 'bg-orange-500' : 'bg-red-500'
                      }`}
                      style={{
                        height: isRecording && !isPaused ? height : '4px',
                        opacity: isPaused ? 0.6 : 1,
                      }}
                    />
                  ))}
                </div>
              </>
            )}
          </>
        )}
      </div>
            {/* {showPlayback && recordingPath && (
        <div className="text-sm text-gray-600 px-4">
          Recording saved to: {recordingPath}
        </div>
      )} */}
    </div>
  );
};