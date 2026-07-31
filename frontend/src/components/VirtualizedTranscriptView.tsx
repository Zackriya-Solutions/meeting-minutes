'use client';

import { useCallback, useRef, useEffect, useState, memo } from "react";
import { useTranscriptStreaming } from "@/hooks/useTranscriptStreaming";
import { motion } from "framer-motion";
import { TranscriptSegmentData, localizeSpeakerLabel, resolveSpeakerLabel } from "@/types";
import { SpeakerRenameDialog } from "./MeetingDetails/SpeakerRenameDialog";
import { useT } from "@/lib/i18n";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { Bubble, BubbleContent } from "./ui/bubble";
import { Message, MessageAvatar, MessageContent, MessageHeader } from "./ui/message";
import {
    MessageScroller,
    MessageScrollerButton,
    MessageScrollerContent,
    MessageScrollerItem,
    MessageScrollerProvider,
    MessageScrollerViewport,
} from "./ui/message-scroller";
import { cn } from "@/lib/utils";
import { avatarGradients } from "@/vendor/deslop/primitives/tokens.js";

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
    /** Optional spacing override for the scroll viewport. */
    viewportClassName?: string;

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

    /** Play the saved recording from a transcript-relative timestamp. */
    onPlayTimestamp?: (seconds: number) => void;
    /** Current saved-audio playback position, used to highlight the active segment. */
    playbackTime?: number | null;
    /** Persist a reviewed correction while retaining the original ASR text. */
    onCorrectTranscript?: (transcriptId: string, correctedText: string) => Promise<void> | void;
}

function isPlaybackSegmentActive(
    segment: TranscriptSegmentData,
    playbackTime: number | null
): boolean {
    if (playbackTime == null) return false;
    const start = segment.timestamp ?? 0;
    // Older imports can lack an end timestamp. Keep a short visual window in
    // that case instead of leaving every later segment highlighted.
    const end = segment.endTime != null && segment.endTime > start
        ? segment.endTime
        : start + 8;
    return playbackTime >= start && playbackTime < end;
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

function speakerInitials(label: string): string {
    const words = label.trim().split(/\s+/).filter(Boolean);
    if (words.length === 0) return '?';

    if (words.length === 1) {
        return Array.from(words[0]).slice(0, 2).join('').toLocaleUpperCase();
    }

    return `${Array.from(words[0])[0] ?? ''}${Array.from(words.at(-1) ?? '')[0] ?? ''}`
        .toLocaleUpperCase();
}

// Memoized transcript segment component
const TranscriptSegment = memo(function TranscriptSegment({
    id,
    text,
    isStreaming,
    highlight = false,
    speakerLabel = null,
    speakerId = null,
    speakerRenamable = false,
    onSpeakerClick,
    playbackActive = false,
    isOwn = false,
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
    onPlayTimestamp?: (timestamp: number) => void;
    playbackActive?: boolean;
    onEdit?: () => void;
    isOwn?: boolean;
}) {
    const t = useT();
    const displayText = cleanStopWords(text) || (text.trim() === '' ? t('[Silence]') : text);

    const align = isOwn ? 'end' : 'start';
    const avatarLabel = speakerLabel ?? t('Speaker');
    const avatarInitials = speakerInitials(avatarLabel);
    // IlyaGrshin/wallet_animations InitialsAvatar assigns one of seven colors by
    // `userId % 7`. Diarized speaker ids preserve that mapping; channel-only
    // transcripts use stable ids for the local and remote sides.
    const avatarUserId = speakerId ?? (isOwn ? 0 : 1);
    const avatarGradient = avatarGradients[
        ((Math.trunc(avatarUserId) % avatarGradients.length) + avatarGradients.length)
        % avatarGradients.length
    ];

    return (
        <Message
            id={`segment-${id}`}
            align={align}
            role="listitem"
            className={cn(
                'mb-3 rounded-lg px-1 py-0.5 transition-colors duration-300',
                highlight && 'bg-primary/10 ring-2 ring-ring',
                playbackActive && !highlight && 'bg-primary/10',
            )}
        >
            <MessageAvatar
                aria-label={avatarLabel}
                title={avatarLabel}
                style={{
                    background: `linear-gradient(180deg, ${avatarGradient.top} 0%, ${avatarGradient.bottom} 100%)`,
                }}
                className={cn(
                    'h-8 w-8 text-sm font-bold text-white',
                    isOwn ? 'ml-2' : 'mr-2',
                )}
            >
                <span aria-hidden="true">{avatarInitials}</span>
            </MessageAvatar>
            <MessageContent className={cn('gap-1', isOwn ? 'items-end' : 'items-start')}>
                {speakerLabel && (
                    <MessageHeader
                        className={cn(
                            'gap-2 px-1 text-muted-foreground',
                            isOwn && 'flex-row-reverse',
                        )}
                    >
                        {speakerRenamable && speakerId != null && onSpeakerClick ? (
                            <button
                                type="button"
                                onClick={() => onSpeakerClick(speakerId)}
                                title={t('Rename speaker')}
                                className="text-[10px] font-medium uppercase leading-tight tracking-wide text-muted-foreground hover:text-primary focus:outline-none"
                            >
                                {speakerLabel}
                            </button>
                        ) : (
                            <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground leading-tight">
                                {speakerLabel}
                            </span>
                        )}
                    </MessageHeader>
                )}

                <Bubble
                    align={align}
                    variant={isOwn || isStreaming ? 'muted' : 'secondary'}
                    className={cn(
                        'max-w-[82%]',
                        isOwn
                            ? 'rounded-[16px_16px_4px_16px]'
                            : 'rounded-[16px_16px_16px_4px]',
                    )}
                >
                    <BubbleContent className="whitespace-pre-wrap px-[15px] py-[11px] text-base leading-relaxed">
                        {displayText}
                    </BubbleContent>
                </Bubble>
            </MessageContent>
        </Message>
    );
});

export const VirtualizedTranscriptView: React.FC<VirtualizedTranscriptViewProps> = ({
    segments,
    isRecording = false,
    enableStreaming = false,
    showConfidence = true,
    disableAutoScroll = false,
    viewportClassName,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
    scrollToTimestamp = null,
    speakersById = null,
    onRenameSpeaker,
    onPlayTimestamp,
    playbackTime = null,
    onCorrectTranscript,
}) => {
    const t = useT();
    // Segment id to briefly highlight after a jump-to-timestamp.
    const [highlightedId, setHighlightedId] = useState<string | null>(null);
    // Diarized speaker being renamed (null when the rename dialog is closed).
    const [renamingSpeaker, setRenamingSpeaker] = useState<{ id: number; name: string } | null>(null);
    const [editingSegment, setEditingSegment] = useState<{ id: string; text: string } | null>(null);
    const [isSavingCorrection, setIsSavingCorrection] = useState(false);

    // Stable so memoized segments don't re-render on every parent render.
    const handleSpeakerClick = useCallback((speakerId: number) => {
        setRenamingSpeaker({ id: speakerId, name: speakersById?.get(speakerId) ?? '' });
    }, [speakersById]);
    // Ensures a given jump target is consumed only once (not re-triggered on paginate).
    const seekConsumedRef = useRef<number | null>(null);
    // Ref for infinite scroll trigger element
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);

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

        // Small delay so the list has laid out before we scroll.
        const scrollTimer = setTimeout(() => {
            document
                .getElementById(`segment-${target.id}`)
                ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
            setHighlightedId(target.id);
        }, 150);
        const clearTimer = setTimeout(() => {
            setHighlightedId((cur) => (cur === target.id ? null : cur));
        }, 3200);
        return () => {
            clearTimeout(scrollTimer);
            clearTimeout(clearTimer);
        };
    }, [scrollToTimestamp, segments]);

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

    return (
        <>
        <MessageScrollerProvider
            autoScroll={!disableAutoScroll}
            defaultScrollPosition={isRecording ? 'end' : 'start'}
            scrollPreviousItemPeek={48}
        >
        <MessageScroller className="h-full">
        <MessageScrollerViewport className={cn("px-4 py-2", viewportClassName)}>
        <MessageScrollerContent className="gap-0">
            <div>
            {segments.length === 0 ? (
                isRecording ? null : (
                    <MessageScrollerItem messageId="transcript-empty">
                        <div className="flex min-h-40 items-center justify-center px-6 text-center text-sm text-muted-foreground">
                            {t('No speech was recognized in this recording')}
                        </div>
                    </MessageScrollerItem>
                )
            ) : (
                <>
                    <div className="space-y-1" role="list">
                        {segments.map((segment) => {
                            const isStreaming = streamingSegmentId === segment.id;
                            // Live chunks from older/in-flight recorder sessions can arrive
                            // before the backend has attached the mic/system channel tag.
                            // In an active recording the safe UI fallback is the local mic:
                            // otherwise the user's own speech is rendered as an incoming
                            // anonymous speaker message until the recording is finalized.
                            const isOwnSegment = segment.speaker === 'mic'
                                || (isRecording && segment.speaker == null);
                            const speakerLabel = isOwnSegment
                                ? t('You')
                                : localizeSpeakerLabel(resolveSpeakerLabel(segment, speakersById), t);

                            return (
                                <MessageScrollerItem
                                    key={segment.id}
                                    messageId={segment.id}
                                    scrollAnchor={isRecording && segment.id === segments.at(-1)?.id}
                                >
                                  <motion.div
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
                                        speakerLabel={speakerLabel}
                                        speakerId={segment.speaker_id}
                                        speakerRenamable={
                                            !!onRenameSpeaker &&
                                            segment.speaker_id != null &&
                                            !!speakersById?.has(segment.speaker_id)
                                        }
                                        onSpeakerClick={handleSpeakerClick}
                                        onPlayTimestamp={onPlayTimestamp}
                                        playbackActive={isPlaybackSegmentActive(segment, playbackTime)}
                                        onEdit={onCorrectTranscript ? () => setEditingSegment({ id: segment.id, text: segment.text }) : undefined}
                                        isOwn={isOwnSegment}
                                    />
                                  </motion.div>
                                </MessageScrollerItem>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger (for small lists that grow) */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-muted-foreground">
                                    <div className="w-4 h-4 border-2 border-border border-t-muted-foreground rounded-full animate-spin" />
                                    <span className="text-sm">{t('Loading more...')}</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-muted-foreground">
                                    {t('Showing')} {loadedCount} {t('of')} {totalCount} {t('segments')}
                                </span>
                            ) : null}
                        </div>
                    )}
                </>
            )}
            </div>
        </MessageScrollerContent>
        </MessageScrollerViewport>
        {segments.length > 0 && <MessageScrollerButton />}
        </MessageScroller>
        </MessageScrollerProvider>

            {/* Rename affordance for diarized speakers (saved meetings only). */}
            {onRenameSpeaker && renamingSpeaker && (
                <SpeakerRenameDialog
                    open={true}
                    currentName={renamingSpeaker.name}
                    onOpenChange={(o) => { if (!o) setRenamingSpeaker(null); }}
                    onRename={(name) => onRenameSpeaker(renamingSpeaker.id, name)}
                />
            )}
            <Dialog open={editingSegment != null} onOpenChange={(open) => { if (!open && !isSavingCorrection) setEditingSegment(null); }}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>{t('Correct transcript')}</DialogTitle>
                    </DialogHeader>
                    <textarea
                        value={editingSegment?.text ?? ''}
                        onChange={(event) => setEditingSegment((current) => current ? { ...current, text: event.target.value } : current)}
                        rows={7}
                        className="w-full resize-y rounded-lg border border-border bg-background p-3 text-sm text-foreground outline-none focus:border-primary/40"
                    />
                    <p className="text-xs text-muted-foreground">
                        {t('The original ASR text is preserved. Repeated corrections may become reviewable terminology suggestions.')}
                    </p>
                    <DialogFooter>
                        <button
                            type="button"
                            disabled={isSavingCorrection}
                            onClick={() => setEditingSegment(null)}
                            className="rounded-md border border-border px-3 py-2 text-sm"
                        >
                            {t('Cancel')}
                        </button>
                        <button
                            type="button"
                            disabled={isSavingCorrection || !editingSegment?.text.trim()}
                            onClick={async () => {
                                if (!editingSegment || !onCorrectTranscript) return;
                                setIsSavingCorrection(true);
                                try {
                                    await onCorrectTranscript(editingSegment.id, editingSegment.text);
                                    setEditingSegment(null);
                                } catch (error) {
                                    // The parent owns the user-facing toast; keep this async event
                                    // handler from producing an unhandled rejection as well.
                                    console.warn('Failed to save transcript correction:', error);
                                } finally {
                                    setIsSavingCorrection(false);
                                }
                            }}
                            className="rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground disabled:opacity-50"
                        >
                            {isSavingCorrection ? t('Saving...') : t('Save correction')}
                        </button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
};
