'use client';

import { useCallback, useRef, useReducer, startTransition, useEffect, useState, memo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { useTranscriptStreaming } from "@/hooks/useTranscriptStreaming";
import { ConfidenceIndicator } from "./ConfidenceIndicator";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { RecordingStatusBar } from "./RecordingStatusBar";
import { motion, AnimatePresence } from "framer-motion";
import { TranscriptSegmentData, resolveSpeakerLabel } from "@/types";
import { SpeakerRenameDialog } from "./MeetingDetails/SpeakerRenameDialog";
import { useT } from "@/lib/i18n";

export interface VirtualizedTranscriptViewProps {
    /** Transcript segments to display */
    segments: TranscriptSegmentData[];
    /** Whether recording is in progress */
    isRecording?: boolean;
    /** Whether recording is paused */
    isPaused?: boolean;
    /** Whether processing/finalizing transcription */
    isProcessing?: boolean;
    /** Whether stopping */
    isStopping?: boolean;
    /** Enable streaming effect for latest segment */
    enableStreaming?: boolean;
    /** Show confidence indicators */
    showConfidence?: boolean;
    /** Completely disable auto-scroll behavior (for meeting details page) */
    disableAutoScroll?: boolean;

    // Pagination props (infinite scroll)
    hasMore?: boolean;
    isLoadingMore?: boolean;
    totalCount?: number;
    loadedCount?: number;
    onLoadMore?: () => void;

    /** When set (seconds), scroll to and briefly highlight the segment at this time.
     *  Powers jump-to-timestamp from search results / RAG citations. */
    scrollToTimestamp?: number | null;

    /** Diarized speaker names (id → display_name); takes precedence over channel tags in labels. */
    speakersById?: Map<number, string> | null;
    /** When provided, diarized speaker labels become clickable to rename them. */
    onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
}

// Threshold for enabling virtualization (below this, use simple rendering)
const VIRTUALIZATION_THRESHOLD = 10;

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
    if (seconds === undefined) return '[--:--]';

    const totalSeconds = Math.floor(seconds);
    const minutes = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;

    return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

// Helper function to remove filler words and repetitions
function cleanStopWords(text: string): string {
    const stopWords = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'];

    let cleanedText = text;
    stopWords.forEach(word => {
        const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
        cleanedText = cleanedText.replace(pattern, ' ');
    });

    return cleanedText.replace(/\s+/g, ' ').trim();
}

// Memoized transcript segment component
const TranscriptSegment = memo(function TranscriptSegment({
    id,
    timestamp,
    text,
    confidence,
    isStreaming,
    showConfidence,
    highlight = false,
    speakerLabel = null,
    speakerId = null,
    speakerRenamable = false,
    onSpeakerClick,
}: {
    id: string;
    timestamp: number;
    text: string;
    confidence?: number;
    isStreaming: boolean;
    showConfidence: boolean;
    highlight?: boolean;
    speakerLabel?: string | null;
    speakerId?: number | null;
    speakerRenamable?: boolean;
    onSpeakerClick?: (speakerId: number) => void;
}) {
    const t = useT();
    const displayText = cleanStopWords(text) || (text.trim() === '' ? t('[Silence]') : text);

    return (
        <div
            id={`segment-${id}`}
            className={`mb-3 rounded-lg transition-colors duration-500 ${highlight ? 'bg-[var(--gold-soft)] ring-2 ring-[var(--gold-ring)] -mx-2 px-2 py-1' : ''}`}
        >
            <div className="flex items-start gap-2">
                <div className="flex flex-col items-start flex-shrink-0 min-w-[50px] mt-1">
                    {speakerLabel && (
                        speakerRenamable && speakerId != null && onSpeakerClick ? (
                            <button
                                type="button"
                                onClick={() => onSpeakerClick(speakerId)}
                                title={t('Rename speaker')}
                                className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg3)] hover:text-[var(--gold)] leading-tight text-left focus:outline-none"
                            >
                                {speakerLabel}
                            </button>
                        ) : (
                            <span className="text-[10px] font-medium uppercase tracking-wide text-[var(--fg3)] leading-tight">
                                {speakerLabel}
                            </span>
                        )
                    )}
                    <Tooltip>
                        <TooltipTrigger>
                            <span className="text-xs text-[var(--fg3)]">
                                {formatRecordingTime(timestamp)}
                            </span>
                        </TooltipTrigger>
                        <TooltipContent>
                            {confidence !== undefined && showConfidence && (
                                <ConfidenceIndicator confidence={confidence} showIndicator={showConfidence} />
                            )}
                        </TooltipContent>
                    </Tooltip>
                </div>
                <div className="flex-1">
                    {isStreaming ? (
                        <div className="bg-[var(--bg-elevated)] border border-[var(--border-subtle)] rounded-lg px-3 py-2">
                            <p className="text-base text-[var(--fg1)] leading-relaxed">{displayText}</p>
                        </div>
                    ) : (
                        <p className="text-base text-[var(--fg1)] leading-relaxed">{displayText}</p>
                    )}
                </div>
            </div>
        </div>
    );
});

export const VirtualizedTranscriptView: React.FC<VirtualizedTranscriptViewProps> = ({
    segments,
    isRecording = false,
    isPaused = false,
    isProcessing = false,
    isStopping = false,
    enableStreaming = false,
    showConfidence = true,
    disableAutoScroll = false,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
    scrollToTimestamp = null,
    speakersById = null,
    onRenameSpeaker,
}) => {
    const t = useT();
    // Create scroll ref first - shared between virtualizer and auto-scroll hook
    const scrollRef = useRef<HTMLDivElement>(null);
    // Segment id to briefly highlight after a jump-to-timestamp.
    const [highlightedId, setHighlightedId] = useState<string | null>(null);
    // Diarized speaker being renamed (null when the rename dialog is closed).
    const [renamingSpeaker, setRenamingSpeaker] = useState<{ id: number; name: string } | null>(null);

    // Stable so memoized segments don't re-render on every parent render.
    const handleSpeakerClick = useCallback((speakerId: number) => {
        setRenamingSpeaker({ id: speakerId, name: speakersById?.get(speakerId) ?? '' });
    }, [speakersById]);
    // Ensures a given jump target is consumed only once (not re-triggered on paginate).
    const seekConsumedRef = useRef<number | null>(null);
    // Ref for infinite scroll trigger element
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);

    // Force re-render without flushSync (avoids React warning)
    const [, rerender] = useReducer((x: number) => x + 1, 0);

    // Setup virtualizer for efficient rendering of large lists
    const virtualizer = useVirtualizer({
        count: segments.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => 60, // Estimated height per segment
        overscan: 10, // Render extra items above/below viewport
        onChange: () => {
            startTransition(() => {
                rerender();
            });
        },
    });

    // Custom hook for auto-scrolling (supports both virtualized and non-virtualized)
    useAutoScroll({
        scrollRef,
        segments,
        isRecording,
        isPaused,
        virtualizer,
        virtualizationThreshold: VIRTUALIZATION_THRESHOLD,
        disableAutoScroll,
    });

    // Streaming text effect hook (typewriter animation for new transcripts)
    const { streamingSegmentId, getDisplayText } = useTranscriptStreaming(
        segments,
        isRecording,
        enableStreaming
    );

    // Jump-to-timestamp: scroll to and briefly highlight the segment at `scrollToTimestamp`
    // (seconds). Consumed once per target so pagination re-renders don't re-trigger it.
    useEffect(() => {
        if (scrollToTimestamp == null || segments.length === 0) return;
        if (seekConsumedRef.current === scrollToTimestamp) return;
        seekConsumedRef.current = scrollToTimestamp;

        // Last loaded segment starting at/before the target time (0.5s tolerance).
        let idx = 0;
        for (let i = 0; i < segments.length; i++) {
            if ((segments[i].timestamp ?? 0) <= scrollToTimestamp + 0.5) idx = i;
            else break;
        }
        const target = segments[idx];
        if (!target) return;

        const useVirt = segments.length >= VIRTUALIZATION_THRESHOLD;
        // Small delay so the list has laid out before we scroll.
        const scrollTimer = setTimeout(() => {
            if (useVirt) {
                virtualizer.scrollToIndex(idx, { align: 'center' });
            } else {
                document
                    .getElementById(`segment-${target.id}`)
                    ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
            }
            setHighlightedId(target.id);
        }, 150);
        const clearTimer = setTimeout(() => {
            setHighlightedId((cur) => (cur === target.id ? null : cur));
        }, 3200);
        return () => {
            clearTimeout(scrollTimer);
            clearTimeout(clearTimer);
        };
    }, [scrollToTimestamp, segments, virtualizer]);

    // Infinite scroll: IntersectionObserver to trigger loading more
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording || segments.length === 0) {
            return;
        }

        const triggerElement = loadMoreTriggerRef.current;
        if (!triggerElement) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
            },
            {
                root: null,
                rootMargin: '100px',
                threshold: 0,
            }
        );

        observer.observe(triggerElement);

        return () => observer.disconnect();
    }, [hasMore, isLoadingMore, onLoadMore, isRecording, segments.length]);

    // Scroll-based fallback for fast scrolling
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording) return;

        const scrollElement = scrollRef.current;
        if (!scrollElement) return;

        let ticking = false;

        const handleScroll = () => {
            if (ticking || isLoadingMore || !hasMore) return;

            ticking = true;
            requestAnimationFrame(() => {
                const { scrollTop, scrollHeight, clientHeight } = scrollElement;
                const scrollBottom = scrollHeight - scrollTop - clientHeight;

                // Trigger load when within 200px of bottom
                if (scrollBottom < 200 && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
                ticking = false;
            });
        };

        scrollElement.addEventListener('scroll', handleScroll, { passive: true });
        return () => scrollElement.removeEventListener('scroll', handleScroll);
    }, [onLoadMore, hasMore, isLoadingMore, isRecording]);

    // Use simple rendering for small lists, virtualization for large lists
    const useVirtualization = segments.length >= VIRTUALIZATION_THRESHOLD;

    return (
        <div ref={scrollRef} className="flex flex-col h-full overflow-y-auto px-4 py-2">
            {/* Recording Status Bar - Sticky at top, always visible when recording */}
            <AnimatePresence>
                {isRecording && (
                    <div className="sticky top-0 z-10 bg-[var(--bg-canvas)] pb-2">
                        <RecordingStatusBar isPaused={isPaused} />
                    </div>
                )}
            </AnimatePresence>

            {/* Content - add padding when recording to prevent overlap */}
            <div className={isRecording ? 'pt-2' : ''}>
            {segments.length === 0 ? (
                // Empty state
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="text-center text-[var(--fg2)] mt-8"
                >
                    {isRecording ? (
                        <>
                            <div className="flex items-center justify-center mb-3">
                                <div className={`w-3 h-3 rounded-full ${isPaused ? 'bg-[var(--gold)]' : 'bg-[var(--gold)] animate-pulse'}`}></div>
                            </div>
                            <p className="text-sm text-[var(--fg2)]">
                                {isPaused ? t('Recording paused') : t('Listening for speech...')}
                            </p>
                            <p className="text-xs mt-1 text-[var(--fg3)]">
                                {isPaused ? t('Click resume to continue recording') : t('Speak to see live transcription')}
                            </p>
                        </>
                    ) : (
                        <>
                            <p className="text-lg font-semibold">{t('Welcome to Memento')}</p>
                            <p className="text-xs mt-1">{t('Select a meeting in the sidebar or start a new recording')}</p>
                            <p className="mx-auto mt-2 max-w-md text-xs text-[var(--fg3)]">
                              {t('For a standup summary: open a meeting, choose Improve → Template → Daily Standup, then generate the summary.')}
                            </p>
                        </>
                    )}
                </motion.div>
            ) : useVirtualization ? (
                // Virtualized rendering for large lists
                <>
                    <div
                        style={{
                            height: virtualizer.getTotalSize(),
                            width: "100%",
                            position: "relative",
                        }}
                    >
                        {virtualizer.getVirtualItems().map((virtualRow) => {
                            const segment = segments[virtualRow.index];
                            const isStreaming = streamingSegmentId === segment.id;

                            return (
                                <div
                                    key={segment.id}
                                    data-index={virtualRow.index}
                                    ref={virtualizer.measureElement}
                                    style={{
                                        position: "absolute",
                                        top: 0,
                                        left: 0,
                                        width: "100%",
                                        transform: `translateY(${virtualRow.start}px)`,
                                    }}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={getDisplayText(segment)}
                                        confidence={segment.confidence}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        highlight={highlightedId === segment.id}
                                        speakerLabel={resolveSpeakerLabel(segment, speakersById)}
                                        speakerId={segment.speaker_id}
                                        speakerRenamable={
                                            !!onRenameSpeaker &&
                                            segment.speaker_id != null &&
                                            !!speakersById?.has(segment.speaker_id)
                                        }
                                        onSpeakerClick={handleSpeakerClick}
                                    />
                                </div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger and loading indicator */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-[var(--fg2)]">
                                    <div className="w-4 h-4 border-2 border-[var(--border-strong)] border-t-[var(--fg2)] rounded-full animate-spin" />
                                    <span className="text-sm">{t('Loading more...')}</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-[var(--fg3)]">
                                    {t('Showing')} {loadedCount} {t('of')} {totalCount} {t('segments')}
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-[var(--fg2)]"
                        >
                            <div className="w-2 h-2 bg-[var(--gold)] rounded-full animate-pulse"></div>
                            <span className="text-sm">{t('Listening...')}</span>
                        </motion.div>
                    )}
                </>
            ) : (
                // Simple rendering for small lists (better animations)
                <>
                    <div className="space-y-1">
                        {segments.map((segment) => {
                            const isStreaming = streamingSegmentId === segment.id;

                            return (
                                <motion.div
                                    key={segment.id}
                                    initial={{ opacity: 0, y: 5 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    transition={{ duration: 0.15 }}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={getDisplayText(segment)}
                                        confidence={segment.confidence}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        highlight={highlightedId === segment.id}
                                        speakerLabel={resolveSpeakerLabel(segment, speakersById)}
                                        speakerId={segment.speaker_id}
                                        speakerRenamable={
                                            !!onRenameSpeaker &&
                                            segment.speaker_id != null &&
                                            !!speakersById?.has(segment.speaker_id)
                                        }
                                        onSpeakerClick={handleSpeakerClick}
                                    />
                                </motion.div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger (for small lists that grow) */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-[var(--fg2)]">
                                    <div className="w-4 h-4 border-2 border-[var(--border-strong)] border-t-[var(--fg2)] rounded-full animate-spin" />
                                    <span className="text-sm">{t('Loading more...')}</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-[var(--fg3)]">
                                    {t('Showing')} {loadedCount} {t('of')} {totalCount} {t('segments')}
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-[var(--fg2)]"
                        >
                            <div className="w-2 h-2 bg-[var(--gold)] rounded-full animate-pulse"></div>
                            <span className="text-sm">{t('Listening...')}</span>
                        </motion.div>
                    )}
                </>
            )}
            </div>

            {/* Rename affordance for diarized speakers (saved meetings only). */}
            {onRenameSpeaker && renamingSpeaker && (
                <SpeakerRenameDialog
                    open={true}
                    currentName={renamingSpeaker.name}
                    onOpenChange={(o) => { if (!o) setRenamingSpeaker(null); }}
                    onRename={(name) => onRenameSpeaker(renamingSpeaker.id, name)}
                />
            )}
        </div>
    );
};
