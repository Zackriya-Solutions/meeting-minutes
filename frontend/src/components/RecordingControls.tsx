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
}

export const RecordingControls: React.FC<RecordingControlsProps> = ({
  isRecording,
  barHeights,
  onRecordingStop,
  onRecordingStart,
  onTranscriptReceived,
  onTranscriptionError,
  isRecordingDisabled,
}) => {
  const [showPlayback, setShowPlayback] = useState(false);
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string>('');
  const [isProcessing, setIsProcessing] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
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
    setIsStarting(true);
    setShowPlayback(false);
    setTranscript(''); // Clear any previous transcript
    
    try {
      await invoke('start_recording');
      console.log('Recording started successfully');
      setIsProcessing(false);
      onRecordingStart();
    } catch (error) {
      console.error('Failed to start recording:', error);
      alert('Failed to start recording. Please check the console for details.');
    } finally {
      setIsStarting(false);
    }
  }, [onRecordingStart, isStarting]);

  const stopRecordingAction = useCallback(async () => {
    console.log('Executing stop recording...');
    try {
      setIsProcessing(true);
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const savePath = `${dataDir}/recording-${timestamp}.wav`;
      
      console.log('Saving recording to:', savePath);
      const result = await invoke('stop_recording', { 
        args: {
          save_path: savePath
        }
      });
      
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
    if (!isRecording || isStarting || isStopping) return;
    
    console.log('Stopping recording...');
    setIsStopping(true);
    
    // Immediately trigger the stop action
    await stopRecordingAction();
  }, [isRecording, isStarting, isStopping, stopRecordingAction]);

  useEffect(() => {
    return () => {
      // Cleanup on unmount if needed
    };
  }, []);

  useEffect(() => {
    console.log('Setting up transcript-error event listener');
    let unsubscribe: (() => void) | undefined;
    
    const setupListener = async () => {
      try {
        unsubscribe = await listen('transcript-error', (event) => {
          console.log('transcript-error event received:', event);
          console.error('Transcription error received:', event.payload);
          const errorMessage = event.payload as string;
          
          // Track the error (no debouncing needed since backend only emits once)
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
        console.log('transcript-error event listener set up successfully');
      } catch (error) {
        console.error('Failed to set up transcript-error event listener:', error);
      }
    };
    
    setupListener();
    
    return () => {
      console.log('Cleaning up transcript-error event listener');
      if (unsubscribe && typeof unsubscribe === 'function') {
        unsubscribe();
      }
    };
  }, []); // Include dependencies

    return (
    <div className="flex flex-col space-y-2">
      <div className="flex items-center space-x-2 bg-white rounded-full shadow-lg px-4 py-2">
        {isProcessing ? (
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
                <button
                  onClick={() => {
                    if (isRecording) {
                      Analytics.trackButtonClick('stop_recording', 'recording_controls');
                      handleStopRecording();
                    } else {
                      Analytics.trackButtonClick('start_recording', 'recording_controls');
                      handleStartRecording();
                    }
                  }}
                  disabled={isStarting || isProcessing || isStopping || isRecordingDisabled}
                  className={`w-12 h-12 flex items-center justify-center ${
                    isStarting || isProcessing || isStopping ? 'bg-gray-400' : 'bg-red-500 hover:bg-red-600'
                  } rounded-full text-white transition-colors relative`}

                >
                  {isRecording ? (
                    <>
                      <Square size={20} />
                      {isStopping && (
                        <div className="absolute -top-8 text-gray-600 font-medium text-sm">
                          Stopping...
                        </div>
                      )}
                    </>
                  ) : (
                    <Mic size={20} />
                  )}
                </button>

                <div className="flex items-center space-x-1 mx-4">
                  {barHeights.map((height, index) => (
                    <div
                      key={index}
                      className="w-1 bg-red-500 rounded-full transition-all duration-200"
                      style={{
                        height: isRecording ? height : '4px',
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