'use client';

import { useCallback, useRef, useReducer, startTransition, useEffect, useState, memo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { useTranscriptStreaming } from "@/hooks/useTranscriptStreaming";
import { ConfidenceIndicator } from "./ConfidenceIndicator";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { RecordingStatusBar } from "./RecordingStatusBar";
import { motion, AnimatePresence } from "framer-motion";
import { NewTranscriptAnnotation, TranscriptAnnotation, TranscriptSegmentData } from "@/types";
import { mergeTranscriptTimeline, TranscriptTimelineItem } from "@/lib/transcript-timeline";
import { formatClipboardImageError, imageDataToDataUrl, readClipboardImageAnnotation, resolveTranscriptAnchor } from "@/lib/transcript-annotation-input";
import { readImage } from "@tauri-apps/plugin-clipboard-manager";
import { Bookmark, ImageIcon, StickyNote } from "lucide-react";

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
    /** Manual transcript annotations */
    annotations?: TranscriptAnnotation[];
    onAddAnnotation?: (annotation: NewTranscriptAnnotation) => Promise<void> | void;
    getAnnotationImage?: (annotation: TranscriptAnnotation) => Promise<string | null>;
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
    source,
    confidence,
    translation,
    translationTargetLanguage,
    translationStatus,
    translationError,
    isStreaming,
    showConfidence,
    isActive,
    onSelectTimestamp,
}: {
    id: string;
    timestamp: number;
    text: string;
    source?: string;
    confidence?: number;
    translation?: string;
    translationTargetLanguage?: string;
    translationStatus?: 'pending' | 'ready' | 'error' | 'disabled';
    translationError?: string;
    isStreaming: boolean;
    showConfidence: boolean;
    isActive: boolean;
    onSelectTimestamp?: (timestamp: number) => void;
}) {
    const displayText = cleanStopWords(text) || (text.trim() === '' ? '[Silence]' : text);
    const speakerLabel = source && source !== 'Audio' ? source : null;
    const speakerLabelClass = speakerLabel === 'Você'
        ? 'bg-blue-100 text-blue-700'
        : 'bg-gray-100 text-gray-600';

    return (
        <div
            id={`segment-${id}`}
            className={`mb-3 cursor-pointer rounded-lg border-l-4 px-2 py-1 transition-colors ${isActive ? 'border-blue-500 bg-blue-50 shadow-sm' : 'border-transparent'}`}
            aria-current={isActive ? 'true' : undefined}
            onClick={() => onSelectTimestamp?.(timestamp)}
        >
            {isActive && (
                <div className="mb-1 flex items-center gap-2 text-[11px] font-medium text-blue-700">
                    <span className="h-2 w-2 rounded-full bg-blue-500" aria-hidden="true" />
                    Insertion point — annotations attach here
                </div>
            )}
            <div className="flex items-start gap-2">
                <Tooltip>
                    <TooltipTrigger>
                        <span className="text-xs text-gray-400 mt-1 flex-shrink-0 min-w-[50px]">
                            {formatRecordingTime(timestamp)}
                        </span>
                    </TooltipTrigger>
                    <TooltipContent>
                        {confidence !== undefined && showConfidence && (
                            <ConfidenceIndicator confidence={confidence} showIndicator={showConfidence} />
                        )}
                    </TooltipContent>
                </Tooltip>
                <div className="flex-1">
                    {speakerLabel && (
                        <span className={`mb-1 inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${speakerLabelClass}`}>
                            {speakerLabel}
                        </span>
                    )}
                    {isStreaming ? (
                        <div className="bg-gray-100 border border-gray-200 rounded-lg px-3 py-2">
                            <p className="text-base text-gray-800 leading-relaxed">{displayText}</p>
                        </div>
                    ) : (
                        <p className="text-base text-gray-800 leading-relaxed">{displayText}</p>
                    )}
                    {translationStatus === 'pending' && (
                        <p className="mt-1 text-sm italic text-gray-400">Translating...</p>
                    )}
                    {translationStatus === 'ready' && translation && (
                        <p className="mt-1 border-l-2 border-gray-200 pl-2 text-sm leading-relaxed text-gray-500">
                            <span className="mr-1 text-[10px] font-medium uppercase text-gray-400">{translationTargetLanguage}</span>
                            {translation}
                        </p>
                    )}
                    {translationStatus === 'error' && (
                        <p className="mt-1 text-sm text-red-500" role="status">
                            Translation unavailable{translationError ? `: ${translationError}` : ''}
                        </p>
                    )}
                </div>
            </div>
        </div>
    );
});

const AnnotationRow = memo(function AnnotationRow({
    annotation,
    getAnnotationImage,
}: {
    annotation: TranscriptAnnotation;
    getAnnotationImage?: (annotation: TranscriptAnnotation) => Promise<string | null>;
}) {
    const [imageSrc, setImageSrc] = useState<string | null>(null);

    useEffect(() => {
        let active = true;
        if (annotation.type === 'image' && annotation.imageData && annotation.imageData.length > 0) {
            setImageSrc(imageDataToDataUrl(annotation.imageData, annotation.imageMime || 'image/png'));
        } else if (annotation.type === 'image' && annotation.imageFile && getAnnotationImage) {
            void getAnnotationImage(annotation).then(src => {
                if (active) setImageSrc(src);
            });
        }
        return () => {
            active = false;
        };
    }, [annotation, getAnnotationImage]);

    return (
        <div className="mb-3 flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2" onClick={(event) => event.stopPropagation()}>
            <span className="mt-1 min-w-[50px] text-xs text-amber-600">{formatRecordingTime(annotation.anchorTime)}</span>
            <div className="min-w-0 flex-1">
                {annotation.type === 'bookmark' && <div className="flex items-center gap-1 text-sm font-medium text-amber-800"><Bookmark size={14} /> Marker</div>}
                {annotation.type === 'note' && <div className="flex items-start gap-1 text-sm text-amber-900"><StickyNote size={14} className="mt-0.5 shrink-0" /><span className="whitespace-pre-wrap break-words">{annotation.text}</span></div>}
                {annotation.type === 'image' && imageSrc && <img src={imageSrc} alt={annotation.text || 'Pasted screenshot'} className="max-h-56 max-w-full rounded border border-amber-200 object-contain" />}
                {annotation.type === 'image' && !imageSrc && <div className="flex items-center gap-1 text-sm text-amber-700"><ImageIcon size={14} /> Loading screenshot…</div>}
            </div>
        </div>
    );
});

function TimelineRow({
    item,
    streamingSegmentId,
    getDisplayText,
    showConfidence,
    activeTimestamp,
    onSelectTimestamp,
    getAnnotationImage,
}: {
    item: TranscriptTimelineItem;
    streamingSegmentId: string | null;
    getDisplayText: (segment: TranscriptSegmentData) => string;
    showConfidence: boolean;
    activeTimestamp: number | null;
    onSelectTimestamp?: (timestamp: number) => void;
    getAnnotationImage?: (annotation: TranscriptAnnotation) => Promise<string | null>;
}) {
    if (item.kind === 'annotation') {
        return <AnnotationRow annotation={item.annotation} getAnnotationImage={getAnnotationImage} />;
    }
    return (
        <TranscriptSegment
            id={item.segment.id}
            timestamp={item.segment.timestamp}
            text={getDisplayText(item.segment)}
            source={item.segment.source}
            confidence={item.segment.confidence}
            translation={item.segment.translation}
            translationTargetLanguage={item.segment.translationTargetLanguage}
            translationStatus={item.segment.translationStatus}
            translationError={item.segment.translationError}
            isStreaming={streamingSegmentId === item.segment.id}
            showConfidence={showConfidence}
            isActive={activeTimestamp === item.segment.timestamp}
            onSelectTimestamp={onSelectTimestamp}
        />
    );
}

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
    annotations = [],
    onAddAnnotation,
    getAnnotationImage,
}) => {
    // Create scroll ref first - shared between virtualizer and auto-scroll hook
    const scrollRef = useRef<HTMLDivElement>(null);
    // Ref for infinite scroll trigger element
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);
    const [activeTimestamp, setActiveTimestamp] = useState<number | null>(null);
    const [noteInput, setNoteInput] = useState('');
    const [pasteError, setPasteError] = useState<string | null>(null);
    const timelineItems = mergeTranscriptTimeline(segments, annotations);

    // Force re-render without flushSync (avoids React warning)
    const [, rerender] = useReducer((x: number) => x + 1, 0);

    // Setup virtualizer for efficient rendering of large lists
    const virtualizer = useVirtualizer({
        count: timelineItems.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => 90, // Original plus optional translation/status line
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
        segments: timelineItems,
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

    const latestSegment = segments[segments.length - 1];
    const { time: anchorTime, isDefault: anchorIsDefault, label: anchorLabel } = resolveTranscriptAnchor(activeTimestamp, latestSegment);
    const canAddAnnotations = Boolean(onAddAnnotation) && !isStopping && !isProcessing;
    const handleSelectTimestamp = useCallback((timestamp: number) => {
        setActiveTimestamp(timestamp);
        scrollRef.current?.focus();
    }, []);
    const addAnnotation = useCallback(async (input: NewTranscriptAnnotation) => {
        if (isStopping || isProcessing) return;
        await onAddAnnotation?.(input);
    }, [isProcessing, isStopping, onAddAnnotation]);
    const handlePaste = useCallback((event: React.ClipboardEvent<HTMLDivElement>) => {
        const item = Array.from(event.clipboardData.items).find(entry => entry.type.startsWith('image/'));
        if (!item || !canAddAnnotations) return;
        const file = item.getAsFile();
        if (!file) return;
        event.preventDefault();
        void file.arrayBuffer().then(buffer => addAnnotation({
            type: 'image',
            anchorTime,
            imageData: Array.from(new Uint8Array(buffer)),
            imageMime: item.type,
        }));
    }, [addAnnotation, anchorTime, canAddAnnotations]);
    const handleClipboardImagePaste = useCallback(async () => {
        if (!canAddAnnotations) return;
        setPasteError(null);
        try {
            const annotation = await readClipboardImageAnnotation(
                readImage,
                anchorTime,
                undefined,
                async () => {
                    const png = await invoke<number[] | null>('read_wayland_clipboard_image');
                    return png ? new Uint8Array(png) : null;
                },
            );
            if (annotation) {
                await addAnnotation(annotation);
            } else {
                setPasteError('Nenhuma imagem no clipboard.');
            }
        } catch (err) {
            setPasteError(formatClipboardImageError(err));
        }
    }, [addAnnotation, anchorTime, canAddAnnotations]);
    const handleGlobalPaste = useCallback((event: KeyboardEvent) => {
        if (!canAddAnnotations || event.key.toLowerCase() !== 'v' || (!event.ctrlKey && !event.metaKey)) return;
        const target = event.target as HTMLElement | null;
        if (target?.closest?.('input, textarea, [contenteditable="true"]')) return;
        event.preventDefault();
        void handleClipboardImagePaste();
    }, [canAddAnnotations, handleClipboardImagePaste]);
    const addBookmark = useCallback(() => {
        void addAnnotation({ type: 'bookmark', anchorTime });
    }, [addAnnotation, anchorTime]);
    const addNote = useCallback(() => {
        const text = noteInput.trim();
        if (!text) return;
        void addAnnotation({ type: 'note', anchorTime, text });
        setNoteInput('');
    }, [addAnnotation, anchorTime, noteInput]);

    useEffect(() => {
        if (!canAddAnnotations) return;
        document.addEventListener('keydown', handleGlobalPaste);
        return () => document.removeEventListener('keydown', handleGlobalPaste);
    }, [canAddAnnotations, handleGlobalPaste]);

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
    const useVirtualization = timelineItems.length >= VIRTUALIZATION_THRESHOLD;

    return (
        <div ref={scrollRef} tabIndex={0} onPaste={handlePaste} className="flex flex-col h-full overflow-y-auto px-4 py-2 outline-none">
            {/* Recording Status Bar - Sticky at top, always visible when recording */}
            <AnimatePresence>
                {isRecording && (
                    <div className="sticky top-0 z-10 bg-white pb-2">
                        <RecordingStatusBar isPaused={isPaused} />
                    </div>
                )}
            </AnimatePresence>

            {canAddAnnotations && (
                <div className="sticky top-0 z-10 mb-3 flex flex-wrap items-center gap-2 border-b border-gray-100 bg-white py-2">
                    <button type="button" onClick={addBookmark} className="inline-flex items-center gap-1 rounded border border-gray-200 px-2 py-1 text-xs text-gray-700 hover:bg-gray-50" title="Add a marker at the selected transcript point"><Bookmark size={13} /> Marker</button>
                    <button type="button" onClick={() => void handleClipboardImagePaste()} className="inline-flex items-center gap-1 rounded border border-gray-200 px-2 py-1 text-xs text-gray-700 hover:bg-gray-50" title="Paste an image from the system clipboard"><ImageIcon size={13} /> Colar imagem</button>
                    <div className="flex min-w-[180px] flex-1 gap-1">
                        <input value={noteInput} onChange={event => setNoteInput(event.target.value)} onPaste={event => event.stopPropagation()} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); addNote(); } if (event.key.toLowerCase() === 'v' && (event.ctrlKey || event.metaKey)) event.stopPropagation(); }} placeholder="Short note…" className="min-w-0 flex-1 rounded border border-gray-200 px-2 py-1 text-xs focus:border-blue-400 focus:outline-none" />
                        <button type="button" onClick={addNote} disabled={!noteInput.trim()} className="inline-flex items-center gap-1 rounded border border-gray-200 px-2 py-1 text-xs text-gray-700 disabled:opacity-40"><StickyNote size={13} /> Add</button>
                    </div>
                    <span className="text-[11px] text-gray-500" aria-live="polite">{anchorLabel}{anchorIsDefault ? ' — click a line to choose another anchor' : ''}</span>
                    {pasteError && <span role="alert" className="basis-full text-xs text-red-600">{pasteError}</span>}
                </div>
            )}

            {/* Content - add padding when recording to prevent overlap */}
            <div className={isRecording ? 'pt-2' : ''}>
            {timelineItems.length === 0 ? (
                // Empty state
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
                                {isPaused ? 'Click resume to continue recording' : 'Speak to see live transcription'}
                            </p>
                        </>
                    ) : (
                        <>
                            <p className="text-lg font-semibold">Welcome to meetily!</p>
                            <p className="text-xs mt-1">Start recording to see live transcription</p>
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
                            const item = timelineItems[virtualRow.index];

                            return (
                                <div
                                    key={item.key}
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
                                    <TimelineRow item={item} streamingSegmentId={streamingSegmentId} getDisplayText={getDisplayText} showConfidence={showConfidence} activeTimestamp={activeTimestamp} onSelectTimestamp={handleSelectTimestamp} getAnnotationImage={getAnnotationImage} />
                                </div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger and loading indicator */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-gray-500">
                                    <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
                                    <span className="text-sm">Loading more...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-gray-400">
                                    Showing {loadedCount} of {totalCount} segments
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
                            className="flex items-center gap-2 mt-4 text-gray-500"
                        >
                            <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>
                            <span className="text-sm">Listening...</span>
                        </motion.div>
                    )}
                </>
            ) : (
                // Simple rendering for small lists (better animations)
                <>
                    <div className="space-y-1">
                        {timelineItems.map((item) => {
                            return (
                                <motion.div
                                    key={item.key}
                                    initial={{ opacity: 0, y: 5 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    transition={{ duration: 0.15 }}
                                >
                                    <TimelineRow item={item} streamingSegmentId={streamingSegmentId} getDisplayText={getDisplayText} showConfidence={showConfidence} activeTimestamp={activeTimestamp} onSelectTimestamp={handleSelectTimestamp} getAnnotationImage={getAnnotationImage} />
                                </motion.div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger (for small lists that grow) */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-gray-500">
                                    <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
                                    <span className="text-sm">Loading more...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-gray-400">
                                    Showing {loadedCount} of {totalCount} segments
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
                            className="flex items-center gap-2 mt-4 text-gray-500"
                        >
                            <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>
                            <span className="text-sm">Listening...</span>
                        </motion.div>
                    )}
                </>
            )}
            </div>
        </div>
    );
};
