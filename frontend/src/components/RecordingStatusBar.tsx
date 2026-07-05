'use client';

import { motion } from 'framer-motion';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useEffect, useState } from 'react';
import { Cpu } from 'lucide-react';

interface RecordingStatusBarProps {
  isPaused?: boolean;
  modelLabel?: string;
}

export const RecordingStatusBar: React.FC<RecordingStatusBarProps> = ({ isPaused = false, modelLabel }) => {
  // Get recording duration from backend-synced context (in seconds)
  // Backend polls every 500ms, providing smooth updates
  const { activeDuration } = useRecordingState();

  // Display state synced from backend
  const [displaySeconds, setDisplaySeconds] = useState(0);

  // Sync with backend duration when it changes (handles refresh/navigation)
  useEffect(() => {
    if (activeDuration !== null) {
      // Round to nearest second to avoid decimal issues
      setDisplaySeconds(Math.floor(activeDuration));
    }
  }, [activeDuration]);

  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: -10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.2 }}
      className="flex flex-wrap items-center gap-2 px-3 py-2 bg-gray-50 rounded-lg mb-2"
    >
      <div className={`w-2 h-2 rounded-full ${isPaused ? 'bg-orange-500' : 'bg-red-500 animate-pulse'}`} />
      <span className={`text-sm ${isPaused ? 'text-orange-700' : 'text-gray-700'}`}>
        {isPaused ? 'Paused' : 'Recording'} • {formatDuration(displaySeconds)}
      </span>
      {modelLabel && (
        <span
          className="inline-flex min-w-0 max-w-full items-center gap-1 rounded border border-gray-200 bg-white px-2 py-0.5 text-xs text-gray-600"
          title={`Transcription model: ${modelLabel}`}
        >
          <Cpu className="h-3 w-3 flex-none" />
          <span className="flex-none font-medium">Model</span>
          <span className="min-w-0 truncate">{modelLabel}</span>
        </span>
      )}
    </motion.div>
  );
};
