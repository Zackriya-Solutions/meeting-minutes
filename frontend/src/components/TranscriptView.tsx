'use client';

import { Transcript } from '@/types';
import { useEffect, useRef, useState } from 'react';
import { ConfidenceIndicator } from './ConfidenceIndicator';

interface TranscriptViewProps {
  transcripts: Transcript[];
}

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
  if (seconds === undefined) return '[--:--]';

  const totalSeconds = Math.floor(seconds);
  const minutes = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;

  return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const prevScrollHeightRef = useRef<number>();
  const isUserAtBottomRef = useRef<boolean>(true);

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
          className={`mb-3 p-3 rounded-lg transition-colors duration-200 ${
            transcript.is_partial
              ? 'bg-gray-50 border-l-4 border-gray-200'
              : 'bg-gray-50 border-l-4 border-gray-200'
          }`}
        >
          <div className="flex justify-between items-center mb-1">
            <div className="flex items-center gap-2">
              <span className="text-xs font-mono text-blue-600 bg-blue-50 px-1.5 py-0.5 rounded">
                {transcript.audio_start_time !== undefined
                  ? formatRecordingTime(transcript.audio_start_time)
                  : transcript.timestamp
                }
              </span>
              {transcript.duration !== undefined && (
                <span className="text-xs text-gray-400">
                  {transcript.duration.toFixed(1)}s
                </span>
              )}
            </div>
            <div className="flex items-center space-x-2">
              {transcript.is_partial && (
                // <span className="text-xs text-blue-600 bg-blue-100 px-2 py-0.5 rounded-full flex items-center gap-1">
                  <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>

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
          <p className={`text-sm text-gray-800`}>
            {(() => {
              const filteredText = transcript.text.replace(/^Thank you\.?\s*$/gi, '').trim();
              return filteredText === '' ? '[Silence]' : filteredText;
            })()}
          </p>
        </div>
      ))}
      {transcripts.length === 0 && (
        <div className="text-center text-gray-500 mt-8">
          <p className="text-sm">No transcripts yet</p>
          <p className="text-xs mt-1">Start recording to see live transcription</p>
        </div>
      )}
    </div>
  );
};
