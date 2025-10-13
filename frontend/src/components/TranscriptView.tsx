'use client';

import { Transcript } from '@/types';
import { useEffect, useRef, useState } from 'react';
import { ConfidenceIndicator } from './ConfidenceIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

interface TranscriptViewProps {
  transcripts: Transcript[];
  isRecording?: boolean;
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

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts, isRecording = false }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const prevScrollHeightRef = useRef<number>();
  const isUserAtBottomRef = useRef<boolean>(true);
  const [speechDetected, setSpeechDetected] = useState(false);

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

  // Smart scrolling - only auto-scroll if user is at bottom
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      const isAtBottom = scrollTop + clientHeight >= scrollHeight - 10; // 10px tolerance
      isUserAtBottomRef.current = isAtBottom;
    };

    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // Only auto-scroll if user was at the bottom before new content
    if (isUserAtBottomRef.current) {
      container.scrollTop = container.scrollHeight;
    }

    prevScrollHeightRef.current = container.scrollHeight;
  }, [transcripts]);

  return (
    <div ref={containerRef} className="h-full overflow-y-auto px-4 py-2">
      {transcripts?.map((transcript, index) => (
        <div
          key={transcript.id ? `${transcript.id}-${index}` : `transcript-${index}`}
          className={`mb-3 p-3 rounded-lg transition-colors duration-200 ${transcript.is_partial
            ? 'bg-gray-50 border-l-4 border-gray-200'
            : 'bg-gray-50 border-l-4 border-gray-200'
            }`}
        >
          <div className="flex justify-end items-center mb-1">
            <div className="items-center space-x-2">
              {transcript.is_partial && (
                // <span className="text-xs text-blue-600 bg-blue-100 px-2 py-0.5 rounded-full flex items-center gap-1">
                <div className="w-2 h-2 justify-end bg-blue-500 rounded-full animate-pulse"></div>
                // </span>
              )}
              {transcript.confidence !== undefined && !transcript.is_partial && (
                <ConfidenceIndicator
                  confidence={transcript.confidence}
                  showIndicator={showConfidence}
                />
              )}
            </div>
          </div>
          <p className={`text-md text-gray-800`}>
            {(() => {
              const filteredText = transcript.text.replace(/^Thank you\.?\s*$/gi, '').trim();
              return filteredText === '' ? '[Silence]' : filteredText;
            })()}
          </p>

          <div className="flex  justify-end items-center gap-2">
            <Tooltip>
              <TooltipTrigger>
                <span className="text-xs text-gray-400">
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
                  </span>
                )}
              </TooltipContent>
            </Tooltip>
          </div>
        </div>
      ))}

      {/* Typing indicator - shows when recording and transcripts exist (always shows after first transcript) */}
      {isRecording && transcripts.length > 0 && (
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
      {transcripts.length === 0 && (
        <div className="text-center text-gray-500 mt-8">
          {isRecording ? (
            <>
              <div className="flex items-center justify-center space-x-2 mb-3">
                <div className="relative flex items-center justify-center">
                  {/* Outer pulse ring - changes color when speech detected */}
                  <div className={`absolute w-12 h-12 rounded-full animate-ping opacity-75 ${speechDetected ? 'bg-green-400' : 'bg-blue-400'
                    }`}></div>
                  {/* Inner solid circle with mic icon */}
                  <div className={`relative w-10 h-10 rounded-full flex items-center justify-center ${speechDetected ? 'bg-green-500' : 'bg-blue-500'
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
              <p className={`text-sm font-medium ${speechDetected ? 'text-green-600' : 'text-blue-600'}`}>
                {speechDetected ? 'Processing speech...' : 'Listening for speech...'}
              </p>
              <p className="text-xs mt-1 text-gray-400">
                {speechDetected
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
