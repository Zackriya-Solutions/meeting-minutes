'use client';

import { invoke } from '@tauri-apps/api/tauri';
import { appDataDir } from '@tauri-apps/api/path';
import { useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';

interface TranscriptUpdate {
  text: string;
  timestamp: string;
}

interface RecordingControlsProps {
  isRecording: boolean;
  barHeights: string[];
  onRecordingStop: () => void;
  onRecordingStart: () => void;
  onTranscriptUpdate: (update: TranscriptUpdate) => void;
}

export const RecordingControls: React.FC<RecordingControlsProps> = ({
  isRecording,
  barHeights,
  onRecordingStop,
  onRecordingStart,
  onTranscriptUpdate,
}) => {
  // Listen for transcript updates
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupListener = async () => {
      unlistenFn = await listen<TranscriptUpdate>('transcript-update', (event) => {
        console.log('Received transcript update:', event.payload);
        onTranscriptUpdate(event.payload);
      });
    };

    setupListener().catch(console.error);

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [onTranscriptUpdate]);

  const handleStartRecording = useCallback(async () => {
    console.log('Starting recording...');
    try {
      await invoke('start_recording');
      console.log('Recording started successfully');
      onRecordingStart();
    } catch (error) {
      console.error('Failed to start recording:', error);
    }
  }, [onRecordingStart]);

  const handleStopRecording = useCallback(async () => {
    console.log('Stopping recording...');
    try {
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const savePath = `${dataDir}/recording-${timestamp}.wav`;
      
      console.log('Saving recording to:', savePath);
      await invoke('stop_recording', { savePath });
      console.log('Recording stopped and saved successfully');
      onRecordingStop();
    } catch (error) {
      console.error('Failed to stop recording:', error);
      onRecordingStop();
    }
  }, [onRecordingStop]);

  console.log('RecordingControls render - isRecording:', isRecording);

  return (
    <div className="flex items-center space-x-3">
      {isRecording ? (
        <>
          {/* Recording animation bars */}
          <div className="flex items-center justify-center space-x-[3px] w-10 h-10">
            <div 
              className="w-[3px] bg-red-500 rounded-full transition-all duration-300 ease-in-out"
              style={{ height: barHeights[0] }}
            />
            <div 
              className="w-[3px] bg-red-500 rounded-full transition-all duration-300 ease-in-out"
              style={{ height: barHeights[1] }}
            />
            <div 
              className="w-[3px] bg-red-500 rounded-full transition-all duration-300 ease-in-out"
              style={{ height: barHeights[2] }}
            />
          </div>
          <button 
            onClick={handleStopRecording}
            className="w-12 h-12 bg-red-50 hover:bg-red-100 rounded-full flex items-center justify-center transition-colors duration-200 border-2 border-red-500"
          >
            <div className="w-4 h-4 bg-red-500 rounded-sm" />
          </button>
        </>
      ) : (
        <>
          {/* Paused indicator */}
          <div className="w-10 h-10 flex items-center justify-center">
            <div className="w-3 h-3 bg-gray-400 rounded-full animate-pulse" />
          </div>
          <button 
            onClick={handleStartRecording}
            className="w-12 h-12 bg-green-50 hover:bg-green-100 rounded-full flex items-center justify-center transition-colors duration-200 border-2 border-green-500"
          >
            <div className="w-0 h-0 border-t-[8px] border-t-transparent border-l-[12px] border-l-green-500 border-b-[8px] border-b-transparent ml-1" />
          </button>
        </>
      )}
    </div>
  );
};