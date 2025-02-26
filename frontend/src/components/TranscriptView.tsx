'use client';
import { Transcript } from '@/types/index';
import { useEffect, useRef } from 'react';

interface TranscriptViewProps {
  transcripts: Transcript[];
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts }) => {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [transcripts]);

  return (
    <div ref={containerRef} className="h-full overflow-y-auto px-4 py-2">
      {transcripts.map((transcript: Transcript) => (
        <div key={transcript.timestamp} className="mb-3 p-2 bg-gray-50 rounded-lg">
          <span className="text-xs text-gray-500 block mb-1">{transcript.timestamp}</span>
          <p className="text-sm text-gray-800">{transcript.text}</p>
        </div>
      ))}
    </div>
  );
};
