'use client';

import { Transcript } from '@/types';
import { useEffect, useRef, useState } from 'react';
import { ConfidenceIndicator } from './ConfidenceIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

interface TranscriptViewProps {
  transcripts: Transcript[];
  isRecording?: boolean;
  isPaused?: boolean; // Is recording paused (affects UI indicators)
  isProcessing?: boolean; // Is processing/finalizing transcription (hides "Listening..." indicator)
  isStopping?: boolean; // Is recording being stopped (provides immediate UI feedback)
  enableStreaming?: boolean; // Enable streaming effect for live transcription UX
}

interface SpeechDetectedEvent {
  message: string;
}

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
  if (seconds === undefined) return '[--:--]';

  const totalSeconds = Math.floor(seconds);
  const minutes = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;

  return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts, isRecording = false, isPaused = false, isProcessing = false, isStopping = false, enableStreaming = false }) => {
  const [speechDetected, setSpeechDetected] = useState(false);

  // Debug: Log the props to understand what's happening
  console.log('TranscriptView render:', {
    isRecording,
    isPaused,
    isProcessing,
    isStopping,
    transcriptCount: transcripts.length,
    shouldShowListening: !isStopping && isRecording && !isPaused && !isProcessing && transcripts.length > 0
  });

  // Streaming effect state
  const [streamingTranscript, setStreamingTranscript] = useState<{
    id: string;
    visibleText: string;
    fullText: string;
  } | null>(null);
  const streamingIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const lastStreamedIdRef = useRef<string | null>(null); // Track which transcript we've streamed

  // Load preference for showing confidence indicator
  const [showConfidence, setShowConfidence] = useState<boolean>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('showConfidenceIndicator');
      return saved !== null ? saved === 'true' : true; // Default to true
    }
    return true;
  });

  // Listen for preference changes from settings
  useEffect(() => {
    const handleConfidenceChange = (e: Event) => {
      const customEvent = e as CustomEvent<boolean>;
      setShowConfidence(customEvent.detail);
    };

    window.addEventListener('confidenceIndicatorChanged', handleConfidenceChange);
    return () => window.removeEventListener('confidenceIndicatorChanged', handleConfidenceChange);
  }, []);

  // Listen for speech-detected event
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;

    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unsubscribe = await listen<SpeechDetectedEvent>('speech-detected', () => {
        setSpeechDetected(true);
      });
    };

    if (isRecording) {
      setupListener();
    } else {
      // Reset when not recording
      setSpeechDetected(false);
    }

    return () => {
      if (unsubscribe) {
        unsubscribe();
      }
    };
  }, [isRecording]);

  // Streaming effect: animate new transcripts character-by-character
  useEffect(() => {
    if (!enableStreaming || !isRecording) {
      // Clean up if streaming is disabled
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }
      setStreamingTranscript(null);
      lastStreamedIdRef.current = null;
      return;
    }

    // Find the latest non-partial transcript
    const latestTranscript = transcripts
      .slice(-1)[0];

    if (!latestTranscript) return;

    // Check if this is a new transcript we haven't streamed yet (using ref to avoid dependency issues)
    if (lastStreamedIdRef.current !== latestTranscript.id) {
      // Clear any existing streaming interval
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }

      // Mark this transcript as being streamed
      lastStreamedIdRef.current = latestTranscript.id;

      const fullText = latestTranscript.text;

      // Calculate speed to complete in exactly 3 seconds
      const TOTAL_DURATION_MS = 3000; // 3 seconds total
      const INTERVAL_MS = 30; // Update every 30ms for smooth animation
      const totalTicks = TOTAL_DURATION_MS / INTERVAL_MS; // 100 ticks
      const charsPerTick = Math.max(1, Math.ceil(fullText.length / totalTicks)); // Calculate chars per tick
      const INITIAL_CHARS = Math.min(3, fullText.length); // Start with first 3 chars visible
      let charIndex = INITIAL_CHARS;

      setStreamingTranscript({
        id: latestTranscript.id,
        visibleText: fullText.substring(0, INITIAL_CHARS),
        fullText: fullText
      });

      streamingIntervalRef.current = setInterval(() => {
        charIndex += charsPerTick;

        if (charIndex >= fullText.length) {
          // Streaming complete
          clearInterval(streamingIntervalRef.current!);
          streamingIntervalRef.current = null;
          setStreamingTranscript(null);
        } else {
          setStreamingTranscript(prev => {
            if (!prev) return null;
            return {
              ...prev,
              visibleText: fullText.substring(0, charIndex)
            };
          });
        }
      }, INTERVAL_MS);
    }
  }, [transcripts, enableStreaming, isRecording]);

  // Cleanup streaming interval on unmount
  useEffect(() => {
    return () => {
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }
      lastStreamedIdRef.current = null;
    };
  }, []);

  return (
    <div className="px-4 py-2">
      {transcripts?.filter(t => t.id !== streamingTranscript?.id).map((transcript, index) => (
        <div
          key={transcript.id ? `${transcript.id}-${index}` : `transcript-${index}`}
          className={`mb-3 p-2 rounded-lg transition-colors duration-200 ${transcript.is_partial
            ? 'bg-gray-50 border-l-4 border-gray-200'
            : 'bg-gray-50 border-l-4 border-gray-200'
            }`}
        >
          <p className={`text-md text-gray-800`}>
            {(() => {
              // Streaming transcript is filtered out and rendered separately, so this is always full text
              const filteredText = transcript.text.replace(/^Thank you\.?\s*$/gi, '').trim();
              return filteredText === '' ? '[Silence]' : filteredText;
            })()}
          </p>

          <div className="flex  justify-end items-center gap-2">
            <Tooltip>
              <TooltipTrigger>
                <span className="text-xs h-min text-gray-400">
                  {transcript.audio_start_time !== undefined
                    ? formatRecordingTime(transcript.audio_start_time)
                    : transcript.timestamp
                  }
                </span>
              </TooltipTrigger>
              <TooltipContent>
                {transcript.duration !== undefined && (
                  <span className="text-xs text-gray-400">
                    {transcript.duration.toFixed(1)}s
                    {transcript.confidence !== undefined && (
                      <ConfidenceIndicator
                        confidence={transcript.confidence}
                        showIndicator={showConfidence}
                      />
                    )}
                  </span>
                )}
              </TooltipContent>
            </Tooltip>
          </div>
        </div>
      ))}

      {/* Streaming transcript - rendered separately with animation */}
      {streamingTranscript && (
        <div
          key={`streaming-${streamingTranscript.id}`}
          className="mb-3 p-3 rounded-lg transition-colors duration-200 bg-blue-50 border-l-4 border-blue-400"
        >
          <p className="text-md text-gray-800">
            {streamingTranscript.visibleText}
          </p>
          <div className="flex justify-end items-center gap-2">
            <span className="text-xs text-blue-400">Transcribing...</span>
          </div>
        </div>
      )}

      {/* Typing indicator - shows when actively recording (not paused, not processing, not stopping) and transcripts exist */}
      {!isStopping && isRecording && !isPaused && !isProcessing && transcripts.length > 0 && (
        <div className="mb-3 p-3 rounded-lg bg-gradient-to-r from-gray-50 to-blue-50/30 border-l-4 border-blue-400 animate-fade-in">
          <div className="flex items-center space-x-2">
            <div className="flex space-x-1">
              <div className="w-2 h-2 bg-blue-500 rounded-full animate-bounce" style={{ animationDelay: '0ms' }}></div>
              <div className="w-2 h-2 bg-blue-500 rounded-full animate-bounce" style={{ animationDelay: '150ms' }}></div>
              <div className="w-2 h-2 bg-blue-500 rounded-full animate-bounce" style={{ animationDelay: '300ms' }}></div>
            </div>
            <span className="text-md text-blue-600 font-medium">Listening...</span>
          </div>
        </div>
      )}

      {/* Pause indicator - shows when recording is paused and transcripts exist */}
      {isRecording && isPaused && transcripts.length > 0 && (
        <div className="mb-3 p-3 rounded-lg bg-gradient-to-r from-gray-50 to-orange-50/30 border-l-4 border-orange-400 animate-fade-in">
          <div className="flex items-center space-x-2">
            <div className="flex space-x-1">
              <div className="w-2 h-2 bg-orange-500 rounded-full"></div>
              <div className="w-2 h-2 bg-orange-500 rounded-full"></div>
              <div className="w-2 h-2 bg-orange-500 rounded-full"></div>
            </div>
            <span className="text-md text-orange-600 font-medium">Recording paused</span>
          </div>
        </div>
      )}
      {transcripts.length === 0 && (
        <div className="text-center text-gray-500 mt-8">
          {isRecording ? (
            <>
              <div className="flex items-center justify-center space-x-2 mb-3">
                <div className="relative flex items-center justify-center">
                  {/* Outer pulse ring - changes color based on state: orange when paused, green when speech detected, blue otherwise */}
                  <div className={`absolute w-12 h-12 rounded-full ${isPaused ? '' : 'animate-ping'} opacity-75 ${isPaused ? 'bg-orange-400' : speechDetected ? 'bg-green-400' : 'bg-blue-400'
                    }`}></div>
                  {/* Inner solid circle with mic icon */}
                  <div className={`relative w-10 h-10 rounded-full flex items-center justify-center ${isPaused ? 'bg-orange-500' : speechDetected ? 'bg-green-500' : 'bg-blue-500'
                    }`}>
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path strokeLinecap="round" strokeLinejoin="round" d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                      <path strokeLinecap="round" strokeLinejoin="round" d="M19 10v2a7 7 0 0 1-14 0v-2" />
                      <line x1="12" y1="19" x2="12" y2="23" />
                      <line x1="8" y1="23" x2="16" y2="23" />
                    </svg>
                  </div>
                </div>
              </div>
              <p className={`text-sm font-medium ${isPaused ? 'text-orange-600' : speechDetected ? 'text-green-600' : 'text-blue-600'
                }`}>
                {isPaused ? 'Recording paused' : speechDetected ? 'Processing speech...' : 'Listening for speech...'}
              </p>
              <p className="text-xs mt-1 text-gray-400">
                {isPaused
                  ? 'Click resume to continue recording'
                  : speechDetected
                    ? 'Your speech is being transcribed'
                    : 'Listening to your microphone and system audio'
                }
              </p>
            </>
          ) : (
            <>
              <p className="text-sm">No transcripts yet</p>
              <p className="text-xs mt-1">Start recording to see live transcription</p>
            </>
          )}
        </div>
      )}
    </div>
  );
};
