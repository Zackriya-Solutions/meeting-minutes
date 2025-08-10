'use client';

import { Transcript } from '@/types';
import { useEffect, useRef } from 'react';

interface TranscriptViewProps {
  transcripts: Transcript[];
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const prevScrollHeightRef = useRef<number>();
  const isUserAtBottomRef = useRef<boolean>(true);

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
          className={`mb-3 p-3 rounded-lg transition-colors duration-200 bg-gray-50 border-l-4 border-gray-200`}
        >
          <div className="flex justify-between items-center mb-1">
            <span className="text-xs text-gray-500">
              {transcript.timestamp}
            </span>
            <div className="flex items-center space-x-2">
              {/* {transcript.is_partial && (
                <span className="text-xs text-blue-600 bg-blue-100 px-2 py-0.5 rounded-full">
                  Partial
                </span>
              )} */}
              {/* {transcript.sequence_id && (
                <span className="text-xs text-gray-400 font-mono">
                  #{transcript.sequence_id}
                </span>
              )} */}
            </div>
          </div>
          <p className={`text-sm text-gray-800`}>
            {transcript.text}
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
