'use client';

import { Transcript, localizeSpeakerLabel, resolveSpeakerLabel } from '@/types';
import { useEffect, useRef, useState } from 'react';
import { ConfidenceIndicator } from './ConfidenceIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';
import { motion } from 'framer-motion';
import { useT } from '@/lib/i18n';
import StreamingText from '@/vendor/deslop/mini-app/StreamingText';

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

// Helper function to remove consecutive word repetitions (especially short words ≤2 letters)
function cleanRepetitions(text: string): string {
  if (!text || text.trim().length === 0) return text;

  const words = text.split(/\s+/);
  const cleanedWords: string[] = [];

  let i = 0;
  while (i < words.length) {
    const currentWord = words[i];
    const currentWordLower = currentWord.toLowerCase();

    // Count consecutive repetitions of the same word
    let repeatCount = 1;
    while (
      i + repeatCount < words.length &&
      words[i + repeatCount].toLowerCase() === currentWordLower
    ) {
      repeatCount++;
    }

    // For short words (≤2 letters), be aggressive: if repeated 2+ times, keep only 1
    // For longer words, keep 1 if repeated 3+ times (less aggressive)
    if (currentWord.length <= 2) {
      // Short words: "I I I I" → "I", "Tu Tu Tu" → "Tu"
      if (repeatCount >= 2) {
        cleanedWords.push(currentWord);
        i += repeatCount;
      } else {
        cleanedWords.push(currentWord);
        i += 1;
      }
    } else {
      // Longer words: keep original unless heavily repeated
      if (repeatCount >= 3) {
        cleanedWords.push(currentWord);
        i += repeatCount;
      } else {
        cleanedWords.push(currentWord);
        i += 1;
      }
    }
  }

  return cleanedWords.join(' ');
}

// Helper function to remove filler words and stop words from transcripts
function cleanStopWords(text: string): string {
  // FIRST: Clean repetitions (especially short words)
  let cleanedText = cleanRepetitions(text);

  // THEN: Remove filler words
  const stopWords = [
    'uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh',
    // 'like', 'you know', 'i mean', 'sort of', 'kind of',
    // 'basically', 'actually', 'literally', 'right',
    // 'thank you', 'thanks'
  ];

  // Remove each stop word (case-insensitive, with word boundaries)
  stopWords.forEach(word => {
    // Match the stop word at word boundaries, with optional punctuation
    const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
    cleanedText = cleanedText.replace(pattern, ' ');
  });

  // Clean up extra whitespace and trim
  cleanedText = cleanedText.replace(/\s+/g, ' ').trim();

  return cleanedText;
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts, isRecording = false, isPaused = false, isProcessing = false, isStopping = false, enableStreaming = false }) => {
  const t = useT();
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

  // New live segments are revealed by the deslop StreamingText component.
  const [streamingTranscriptId, setStreamingTranscriptId] = useState<string | null>(null);
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

  // Mark each new transcript once; the component handles word-by-word reveal.
  useEffect(() => {
    if (!enableStreaming || !isRecording) {
      setStreamingTranscriptId(null);
      lastStreamedIdRef.current = null;
      return;
    }

    // Find the latest non-partial transcript
    const latestTranscript = transcripts
      .slice(-1)[0];

    if (!latestTranscript) return;

    // Check if this is a new transcript we haven't streamed yet (using ref to avoid dependency issues)
    if (lastStreamedIdRef.current !== latestTranscript.id) {
      lastStreamedIdRef.current = latestTranscript.id;
      setStreamingTranscriptId(latestTranscript.id);
    }
  }, [transcripts, enableStreaming, isRecording]);

  // Forget the last segment on unmount so a future recording starts cleanly.
  useEffect(() => {
    return () => {
      lastStreamedIdRef.current = null;
    };
  }, []);

  return (
    <div className="px-4 py-2">
      {transcripts?.map((transcript, index) => {
        const isStreaming = streamingTranscriptId === transcript.id;
        const textToShow = transcript.text;
        // Clean up text for display - remove repetitions and filler words
        const filteredText = cleanStopWords(textToShow);
        // Show [Silence] ONLY if the ORIGINAL transcript was empty (not just after filtering)
        const originalWasEmpty = transcript.text.trim() === '';
        const displayText = originalWasEmpty && !isStreaming ? t('[Silence]') : filteredText;

        // Sizer text: use cleaned version for proper sizing, fallback to [Silence] only if original was empty
        const sizerText = cleanStopWords(transcript.text)
          || (originalWasEmpty && !isStreaming ? t('[Silence]') : '');

        // Speaker attribution label ("You" for mic, "Others" for system audio)
        const speakerLabel = localizeSpeakerLabel(resolveSpeakerLabel(transcript), t);

        return (
          <motion.div
            key={transcript.id ? `${transcript.id}-${index}` : `transcript-${index}`}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.15 }}
            className="mb-3"
          >
            <div className="flex items-start gap-2">
              <div className="flex flex-col items-start flex-shrink-0 min-w-[50px] mt-1">
                {speakerLabel && (
                  <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground leading-tight">
                    {speakerLabel}
                  </span>
                )}
                <Tooltip>
                  <TooltipTrigger>
                    <span className="text-xs text-muted-foreground">
                      {transcript.audio_start_time !== undefined
                        ? formatRecordingTime(transcript.audio_start_time)
                        : transcript.timestamp}
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>
                    {transcript.duration !== undefined && (
                      <span className="text-xs text-muted-foreground">
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
              <div className="flex-1">
                {isStreaming ? (
                  <div className="rounded-lg border border-border bg-muted px-3 py-2 text-base leading-relaxed text-foreground">
                    <StreamingText mode="word" speed="fast" replayKey={transcript.id}>
                      {displayText}
                    </StreamingText>
                  </div>
                ) : (
                  // Regular transcript - simple text
                  <div className="relative">
                    <p className="text-base text-foreground leading-relaxed" style={{ visibility: 'hidden' }}>
                      {sizerText}
                    </p>
                    <p className="text-base text-foreground leading-relaxed absolute top-0 left-0">
                      {displayText}
                    </p>
                  </div>
                )}
              </div>
            </div>
          </motion.div>
        );
      })}
    </div>
  );
};
