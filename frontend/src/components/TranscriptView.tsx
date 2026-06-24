'use client';

import { Transcript } from '@/types';
import { useEffect, useRef, useState } from 'react';
import { ConfidenceIndicator } from './ConfidenceIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';
import { RecordingStatusBar } from './RecordingStatusBar';
import { motion, AnimatePresence } from 'framer-motion';
import {
  LangView,
  LANG_LABEL,
  LANG_ORDER,
  targetForView,
  translateSegment,
  alreadyTarget,
  copyText,
} from '@/lib/meetLog';

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

  // ── Real-time translation (Original/Thai/English/Bilingual) + copy ───────
  const [langView, setLangView] = useState<LangView>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('meetLogLangView') as LangView | null;
      if (saved && LANG_ORDER.includes(saved)) return saved;
    }
    return 'original';
  });
  const [langMenuOpen, setLangMenuOpen] = useState(false);
  // translations[id] = { th?, en? } per-segment, per-target
  const [translations, setTranslations] = useState<Record<string, { th?: string; en?: string }>>({});
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<NodeJS.Timeout | null>(null);

  const chooseView = (v: LangView) => {
    setLangView(v);
    setLangMenuOpen(false);
    if (typeof window !== 'undefined') localStorage.setItem('meetLogLangView', v);
  };

  const showToast = (msg: string) => {
    setToast(msg);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 1400);
  };

  const handleCopy = async (text: string, label = 'Copied') => {
    const ok = await copyText(text);
    showToast(ok ? label : 'Copy failed');
  };

  // Translate finalized segments into the active target as needed.
  useEffect(() => {
    const target = targetForView(langView);
    if (!target) return;
    let cancelled = false;
    (async () => {
      for (const t of transcripts) {
        if (t.is_partial) continue;
        const text = (t.text || '').trim();
        if (!text || alreadyTarget(text, target)) continue;
        if (translations[t.id]?.[target] !== undefined) continue;
        const out = await translateSegment(text, target);
        if (cancelled) return;
        setTranslations((prev) => {
          if (prev[t.id]?.[target] !== undefined) return prev;
          return { ...prev, [t.id]: { ...prev[t.id], [target]: out } };
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [langView, transcripts, translations]);

  // Translation shown for a transcript given the current view (or undefined).
  const translatedFor = (t: Transcript): string | undefined => {
    const target = targetForView(langView);
    if (!target) return undefined;
    const text = (t.text || '').trim();
    if (alreadyTarget(text, target)) return undefined;
    return translations[t.id]?.[target];
  };

  // Build a markdown copy of the whole transcript honoring the current view.
  const copyAll = async () => {
    const lines = transcripts.map((t) => {
      const ts = t.audio_start_time !== undefined
        ? formatRecordingTime(t.audio_start_time)
        : `[${t.timestamp}]`;
      const original = (t.text || '').trim();
      const tr = translatedFor(t);
      if (langView === 'bilingual' && tr && tr !== original) {
        return `${ts} ${original}\n        ↳ ${tr}`;
      }
      if (langView === 'thai' || langView === 'english') return `${ts} ${tr ?? original}`;
      return `${ts} ${original}`;
    });
    await handleCopy(lines.join('\n'), 'Transcript copied');
  };

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

      // Fast typewriter effect - complete in 0.8 seconds for snappy feel
      const TOTAL_DURATION_MS = 800; // 0.8 seconds total - fast and snappy!
      const INTERVAL_MS = 15; // Update every 15ms for smooth animation
      const totalTicks = TOTAL_DURATION_MS / INTERVAL_MS; // ~53 ticks
      const charsPerTick = Math.max(2, Math.ceil(fullText.length / totalTicks)); // At least 2 chars per tick for speed
      const INITIAL_CHARS = Math.min(5, fullText.length); // Start with first 5 chars visible
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
      {/* Meet-log toolbar: language dropdown + แปลเป็นไทย + copy-all */}
      {transcripts.length > 0 && (
        <div className="sticky top-0 z-20 flex items-center justify-between bg-white/90 backdrop-blur py-2 mb-1">
          <span className="text-xs font-medium text-gray-500">Transcript</span>
          <div className="flex items-center gap-2">
            {/* Quick action: translate everything to Thai (bilingual) */}
            <button
              onClick={() => chooseView('bilingual')}
              title="แปลทั้ง transcript เป็นไทย (แสดงคู่กับต้นฉบับ)"
              className={`rounded-md border px-2 py-1 text-xs ${
                langView === 'bilingual'
                  ? 'border-blue-500 bg-blue-50 text-blue-600'
                  : 'border-gray-200 text-gray-600 hover:bg-gray-50'
              }`}
            >
              🇹🇭 แปลเป็นไทย
            </button>

            {/* Language dropdown */}
            <div className="relative">
              <button
                onClick={() => setLangMenuOpen((o) => !o)}
                title="Choose transcript language"
                className="flex items-center gap-1 rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-600 hover:bg-gray-50"
              >
                <span className="font-semibold">Language:</span>
                <span className="text-gray-700">{LANG_LABEL[langView]}</span>
                <span className="text-gray-400">▾</span>
              </button>
              {langMenuOpen && (
                <div className="absolute right-0 z-30 mt-1 w-36 overflow-hidden rounded-md border border-gray-200 bg-white shadow-lg">
                  {LANG_ORDER.map((v) => (
                    <button
                      key={v}
                      onClick={() => chooseView(v)}
                      className={`block w-full px-3 py-1.5 text-left text-xs hover:bg-gray-50 ${
                        v === langView ? 'bg-blue-50 text-blue-600' : 'text-gray-700'
                      }`}
                    >
                      {LANG_LABEL[v]}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <button
              onClick={copyAll}
              title="Copy entire transcript"
              className="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-600 hover:bg-gray-50"
            >
              ⧉ Copy all
            </button>
          </div>
        </div>
      )}

      {/* Copy toast */}
      <AnimatePresence>
        {toast && (
          <motion.div
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            className="fixed top-4 left-1/2 z-50 -translate-x-1/2 rounded-md bg-gray-900 px-3 py-1.5 text-xs text-white shadow-lg"
          >
            {toast}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Recording Status Bar - Sticky at top, always visible when recording */}
      <AnimatePresence>
        {isRecording && (
          <div className="sticky top-4 z-10 bg-white pb-2">
            <RecordingStatusBar isPaused={isPaused} />
          </div>
        )}
      </AnimatePresence>

      {transcripts?.map((transcript, index) => {
        const isStreaming = streamingTranscript?.id === transcript.id;
        const textToShow = isStreaming ? streamingTranscript.visibleText : transcript.text;
        // Clean up text for display - remove repetitions and filler words
        const filteredText = cleanStopWords(textToShow);
        // Show [Silence] ONLY if the ORIGINAL transcript was empty (not just after filtering)
        const originalWasEmpty = transcript.text.trim() === '';
        const displayText = originalWasEmpty && !isStreaming ? '[Silence]' : filteredText;

        // Sizer text: use cleaned version for proper sizing, fallback to [Silence] only if original was empty
        const sizerText = cleanStopWords(isStreaming ? streamingTranscript.fullText : transcript.text)
          || (originalWasEmpty && !isStreaming ? '[Silence]' : '');

        return (
          <motion.div
            key={transcript.id ? `${transcript.id}-${index}` : `transcript-${index}`}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.15 }}
            className="mb-3"
          >
            <div className="group flex items-start gap-2">
              <Tooltip>
                <TooltipTrigger>
                  <span className="text-xs text-gray-400 mt-1 flex-shrink-0 min-w-[50px]">
                    {transcript.audio_start_time !== undefined
                      ? formatRecordingTime(transcript.audio_start_time)
                      : transcript.timestamp}
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
              <div className="flex-1">
                {isStreaming ? (
                  // Streaming transcript - show in bubble (full width)
                  <div className="bg-gray-100 border border-gray-200 rounded-lg px-3 py-2">
                    <div className="relative">
                      <p className="text-base text-gray-800 leading-relaxed" style={{ visibility: 'hidden' }}>
                        {sizerText}
                      </p>
                      <p className="text-base text-gray-800 leading-relaxed absolute top-0 left-0">
                        {displayText}
                      </p>
                    </div>
                  </div>
                ) : (() => {
                  // Regular transcript with translation view + per-line copy.
                  const translated = translatedFor(transcript);
                  const replaceMain = (langView === 'thai' || langView === 'english')
                    && !!translated && translated.trim().length > 0;
                  const mainText = replaceMain ? translated : displayText;
                  const sizerMain = replaceMain ? translated : sizerText;
                  // partial = faded/italic, final = solid
                  const isFinal = !transcript.is_partial;
                  const textColor = isFinal ? 'text-gray-800' : 'text-gray-400 italic';
                  const copyMain = replaceMain ? translated : transcript.text;
                  return (
                    <div>
                      <div className="relative flex items-start gap-1">
                        <div className="relative flex-1">
                          <p className={`text-base ${textColor} leading-relaxed`} style={{ visibility: 'hidden' }}>
                            {sizerMain}
                          </p>
                          <p className={`text-base ${textColor} leading-relaxed absolute top-0 left-0`}>
                            {mainText}
                          </p>
                        </div>
                        <button
                          onClick={() => handleCopy(copyMain)}
                          title="Copy line"
                          className="opacity-0 group-hover:opacity-100 transition-opacity text-gray-300 hover:text-gray-600 text-xs mt-1 flex-shrink-0"
                        >
                          ⧉
                        </button>
                      </div>
                      {langView === 'bilingual' && !!translated && translated !== transcript.text && (
                        <div className="flex items-start gap-1 mt-0.5">
                          <p className="flex-1 text-sm text-blue-600/80 leading-relaxed">🇹🇭 {translated}</p>
                          <button
                            onClick={() => handleCopy(translated, 'แปลแล้ว — คัดลอก')}
                            title="Copy translation"
                            className="opacity-0 group-hover:opacity-100 transition-opacity text-gray-300 hover:text-gray-600 text-xs flex-shrink-0"
                          >
                            ⧉
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })()}
              </div>
            </div>
          </motion.div>
        );
      })}

      {/* Show listening indicator when recording and has transcripts */}
      {!isStopping && isRecording && !isPaused && !isProcessing && transcripts.length > 0 && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="flex items-center gap-2 mt-4 text-gray-500"
        >
          <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>
          <span className="text-sm">Listening...</span>
        </motion.div>
      )}

      {/* Empty state when no transcripts */}
      {transcripts.length === 0 && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="text-center text-gray-500 mt-8"
        >
          {isRecording ? (
            <>
              <div className="flex items-center justify-center mb-3">
                <div className={`w-3 h-3 rounded-full ${isPaused ? 'bg-orange-500' : 'bg-blue-500 animate-pulse'}`}></div>
              </div>
              <p className="text-sm text-gray-600">
                {isPaused ? 'Recording paused' : 'Listening for speech...'}
              </p>
              <p className="text-xs mt-1 text-gray-400">
                {isPaused
                  ? 'Click resume to continue recording'
                  : 'Speak to see live transcription'}
              </p>
            </>
          ) : (
            <>
              <p className="text-lg font-semibold">Welcome to meetily!</p>
              <p className="text-xs mt-1">Start recording to see live transcription</p>
            </>
          )}
        </motion.div>
      )}
    </div>
  );
};
