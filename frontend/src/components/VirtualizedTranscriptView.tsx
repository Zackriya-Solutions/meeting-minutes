'use client';

import { useCallback, useRef, useEffect, useMemo, useState, memo } from "react";
import { toast } from "sonner";
import { useTranscriptStreaming } from "@/hooks/useTranscriptStreaming";
import { TranscriptSegmentData, localizeSpeakerLabel, resolveSpeakerLabel } from "@/types";
import { SpeakerRenameDialog } from "./MeetingDetails/SpeakerRenameDialog";
import { useT } from "@/lib/i18n";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { Bubble, BubbleContent } from "./ui/bubble";
import { Message, MessageAvatar, MessageContent } from "./ui/message";
import { ProceduralSpeakerAvatar } from "./MeetingConversation/ProceduralSpeakerAvatar";
import type { TranscriptReaction } from "./MeetingConversation/ProceduralSpeakerAvatar";
import { useTranscriptReactions } from "@/hooks/useTranscriptReactions";
import {
    MessageScroller,
    MessageScrollerButton,
    MessageScrollerContent,
    MessageScrollerItem,
    MessageScrollerProvider,
    MessageScrollerViewport,
    useMessageScrollerScrollable,
} from "./ui/message-scroller";
import { cn } from "@/lib/utils";
import StreamingText from "@/vendor/deslop/mini-app/StreamingText";
import { Icon } from "@/components/memento/Icon";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { RadioGroup, RadioItem } from "@/components/ui/fluid-radio-group";
import { Input as FluidInput } from "@/components/ui/fluid-input";
import { Button as FluidButton } from "@/components/ui/fluid-button";
import { ShapeProvider } from "@/lib/shape-context";

const ROLL_CALL_TIP_DISMISSED_KEY = 'memento:roll-call-tip-dismissed:v1';
const ADD_NEW_SPEAKER_VALUE = '__add-new-speaker__';

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
    /** Reports whether content continues above or below the visible viewport. */
    onScrollEdgesChange?: (edges: { start: boolean; end: boolean }) => void;

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
    /** Diarized profile ids explicitly confirmed as the local user. */
    selfSpeakerIds?: ReadonlySet<number> | null;
    /** When provided, diarized speaker labels become clickable to rename them. */
    onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
    /** Mark or unmark a diarized voice profile as the local user. */
    onSetSelfSpeaker?: (speakerId: number, isSelf: boolean) => Promise<void> | void;
    /** Attribute a line diarization could not attribute to a voice the user picks. */
    onAssignSegmentSpeaker?: (transcriptId: string, speakerId: number) => Promise<void> | void;
    /** Add a named person and attribute the current line to them. */
    onAddAndAssignSegmentSpeaker?: (transcriptId: string, displayName: string) => Promise<void> | void;

    /** Play the saved recording from a transcript-relative timestamp. */
    onPlayTimestamp?: (seconds: number) => void;
    /** Current saved-audio playback position, used to highlight the active segment. */
    playbackTime?: number | null;
    /** Persist a reviewed correction while retaining the original ASR text. */
    onCorrectTranscript?: (transcriptId: string, correctedText: string) => Promise<void> | void;
}

function ScrollEdgesObserver({
    onChange,
}: {
    onChange?: (edges: { start: boolean; end: boolean }) => void;
}) {
    const edges = useMessageScrollerScrollable();

    useEffect(() => {
        onChange?.({ start: edges.start, end: edges.end });
    }, [edges.end, edges.start, onChange]);

    return null;
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

// Compiled once at module scope: this runs per segment, and rebuilding eight RegExp
// objects on every row of a long transcript was pure allocation churn.
const STOP_WORD_PATTERNS = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'].map(
    (word) => new RegExp(`\\b${word}\\b[,\\s]*`, 'gi')
);
const WHITESPACE_RUN = /\s+/g;

// Helper function to remove filler words and repetitions
function cleanStopWords(text: string): string {
    let cleanedText = text;
    for (const pattern of STOP_WORD_PATTERNS) {
        cleanedText = cleanedText.replace(pattern, ' ');
    }

    return cleanedText.replace(WHITESPACE_RUN, ' ').trim();
}

function formatRecognitionTime(value?: string | null): string | null {
    const source = value?.trim();
    if (!source) return null;

    // Live recognition stores a wall-clock value such as 14:30:05. Saved/imported
    // transcripts may carry an ISO date instead, which has to be shown in local time.
    const plainClock = source.match(/^(\d{1,2}):(\d{2})(?::\d{2}(?:\.\d+)?)?$/);
    if (plainClock) return `${plainClock[1].padStart(2, '0')}:${plainClock[2]}`;

    const date = new Date(source);
    if (!Number.isNaN(date.getTime())) {
        return new Intl.DateTimeFormat(undefined, {
            hour: '2-digit',
            minute: '2-digit',
        }).format(date);
    }

    const embeddedClock = source.match(/(?:^|[T\s])(\d{1,2}):(\d{2})/);
    return embeddedClock
        ? `${embeddedClock[1].padStart(2, '0')}:${embeddedClock[2]}`
        : null;
}

function RollCallTip({ onDismiss }: { onDismiss: () => void }) {
    const t = useT();

    return (
        <Message align="start" role="listitem" className="mb-3 gap-0">
            <MessageAvatar
                aria-label="Memento"
                title="Memento"
                className="h-[60px] w-[60px] min-w-[60px] rounded-none bg-transparent"
            >
                <ProceduralSpeakerAvatar speakerId={8} preset="sphere" />
            </MessageAvatar>
            <MessageContent className="items-start">
                <Bubble
                    align="start"
                    variant="muted"
                    className="max-w-[82%] rounded-[16px_16px_16px_4px]"
                >
                    <BubbleContent className="!bg-[var(--primary-5)] px-[15px] py-[11px] text-[var(--deslop-primary)]">
                        <div className="flex items-start gap-3 text-left">
                            <div className="min-w-0 flex-1">
                                <div className="mb-1 text-xs font-medium text-[var(--deslop-primary-60)]">
                                    Memento
                                </div>
                                <p className="text-base leading-relaxed">
                                    {t('Have everyone say their name at the start of the meeting. This helps me remember voices and recognize participants more accurately.')}
                                </p>
                            </div>
                            <button
                                type="button"
                                onClick={onDismiss}
                                aria-label={t('Dismiss')}
                                title={t('Dismiss')}
                                className="-mr-1 -mt-1 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[var(--deslop-primary-60)] transition-colors hover:bg-[var(--primary-8)] hover:text-[var(--deslop-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-20)]"
                            >
                                <Icon name="close" size={16} />
                            </button>
                        </div>
                    </BubbleContent>
                </Bubble>
            </MessageContent>
        </Message>
    );
}

// Memoized transcript segment component
// Every prop here must be stable across parent renders or `memo` is decorative: a row
// that receives a freshly built callback re-renders even when nothing it displays moved.
// That is why the props are exactly what the row draws — nothing is accepted and ignored.
const TranscriptSegment = memo(function TranscriptSegment({
    id,
    text,
    recognizedAt = null,
    isStreaming,
    highlight = false,
    speakerLabel = null,
    speakerId = null,
    speakerRenamable = false,
    onSpeakerClick,
    onAssignClick,
    playbackActive = false,
    isOwn = false,
    reaction = 'neutral',
}: {
    id: string;
    text: string;
    recognizedAt?: string | null;
    isStreaming: boolean;
    highlight?: boolean;
    speakerLabel?: string | null;
    speakerId?: number | null;
    speakerRenamable?: boolean;
    onSpeakerClick?: (speakerId: number) => void;
    /** Present when this line has no speaker and the user can choose or add a speaker. */
    onAssignClick?: () => void;
    playbackActive?: boolean;
    isOwn?: boolean;
    reaction?: TranscriptReaction;
}) {
    const t = useT();
    const displayText = cleanStopWords(text) || (text.trim() === '' ? t('[Silence]') : text);
    const recognitionTime = formatRecognitionTime(recognizedAt);

    const align = isOwn ? 'end' : 'start';
    // A live segment has no diarized profile yet, so there is no stable person name to
    // render. Reuse the first generated animal label instead of exposing the technical
    // "unrecognized voice" state; once diarization resolves the line, the real animal
    // placeholder or confirmed name replaces it through `speakerLabel`.
    const avatarLabel = speakerLabel ?? localizeSpeakerLabel('Speaker 1', t) ?? t('Unrecognized voice');
    const visibleSpeakerLabel = avatarLabel;
    const avatarUserId = speakerId ?? (isOwn ? 0 : 1);

    return (
        <Message
            id={`segment-${id}`}
            align={align}
            role="listitem"
            className={cn(
                'mb-3 gap-0 rounded-lg px-1 py-0.5 transition-colors duration-300',
                playbackActive && !highlight && 'bg-primary/10',
            )}
        >
            <MessageAvatar
                aria-label={avatarLabel}
                title={avatarLabel}
                className="h-[60px] w-[60px] min-w-[60px] rounded-none bg-transparent"
            >
                <ProceduralSpeakerAvatar speakerId={avatarUserId} reaction={reaction} />
            </MessageAvatar>
            <MessageContent className={cn('gap-0.5', isOwn ? 'items-end' : 'items-start')}>
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
                    <BubbleContent
                        className={cn(
                            'whitespace-pre-wrap px-[15px] py-[11px] text-base leading-relaxed transition-[background-color] duration-500 ease-[cubic-bezier(0.2,0,0,1)]',
                            highlight && '!bg-[var(--primary-10)]',
                        )}
                    >
                        <div className="flex flex-col items-start gap-0.5 text-left">
                            {(
                                speakerRenamable && speakerId != null && onSpeakerClick ? (
                                    <Tooltip>
                                        <TooltipTrigger asChild>
                                            <button
                                                type="button"
                                                onClick={() => onSpeakerClick(speakerId)}
                                                className="speaker-rename-underline text-xs font-normal leading-4 text-[var(--deslop-primary-60)] hover:text-[var(--deslop-primary)] focus:outline-none"
                                            >
                                                {avatarLabel}
                                            </button>
                                        </TooltipTrigger>
                                        <TooltipContent>{t('Rename')}</TooltipContent>
                                    </Tooltip>
                                ) : onAssignClick ? (
                                    // Diarization refused this line; the person reading the
                                    // transcript usually knows who said it.
                                    <Tooltip>
                                        <TooltipTrigger asChild>
                                            <button
                                                type="button"
                                                onClick={onAssignClick}
                                                className="speaker-rename-underline text-xs font-normal leading-4 text-[var(--deslop-primary-60)] hover:text-[var(--deslop-primary)] focus:outline-none"
                                            >
                                                {visibleSpeakerLabel}
                                            </button>
                                        </TooltipTrigger>
                                        <TooltipContent>{t('Choose who said this')}</TooltipContent>
                                    </Tooltip>
                                ) : (
                                    <span
                                        className="text-xs font-normal leading-4 text-[var(--deslop-primary-60)]"
                                    >
                                        {visibleSpeakerLabel}
                                    </span>
                                )
                            )}
                            {isStreaming ? (
                                <StreamingText
                                    mode="word"
                                    speed="fast"
                                    replayKey={id}
                                >
                                    {displayText}
                                </StreamingText>
                            ) : (
                                <span>{displayText}</span>
                            )}
                            {recognitionTime && (
                                <span className="mm-numeric mt-1 self-end text-[10px] leading-none text-[var(--deslop-primary-50)]">
                                    {recognitionTime}
                                </span>
                            )}
                        </div>
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
    onScrollEdgesChange,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
    scrollToTimestamp = null,
    speakersById = null,
    selfSpeakerIds = null,
    onRenameSpeaker,
    onSetSelfSpeaker,
    onAssignSegmentSpeaker,
    onAddAndAssignSegmentSpeaker,
    onPlayTimestamp,
    playbackTime = null,
    onCorrectTranscript,
}) => {
    const t = useT();
    // Segment id to briefly highlight after a jump-to-timestamp.
    const [highlightedId, setHighlightedId] = useState<string | null>(null);
    // Diarized speaker being renamed (null when the rename dialog is closed).
    const [renamingSpeaker, setRenamingSpeaker] = useState<{ id: number; name: string } | null>(null);
    const [assigningSegmentId, setAssigningSegmentId] = useState<string | null>(null);
    const [assigningChoice, setAssigningChoice] = useState<string | null>(null);
    const [newSpeakerName, setNewSpeakerName] = useState('');
    const [isAssigningSpeaker, setIsAssigningSpeaker] = useState(false);
    const [editingSegment, setEditingSegment] = useState<{ id: string; text: string } | null>(null);
    const [isSavingCorrection, setIsSavingCorrection] = useState(false);
    const [showRollCallTip, setShowRollCallTip] = useState(false);

    useEffect(() => {
        setShowRollCallTip(window.localStorage.getItem(ROLL_CALL_TIP_DISMISSED_KEY) !== 'true');
    }, []);

    const dismissRollCallTip = useCallback(() => {
        window.localStorage.setItem(ROLL_CALL_TIP_DISMISSED_KEY, 'true');
        setShowRollCallTip(false);
    }, []);

    const closeAssignDialog = useCallback(() => {
        if (isAssigningSpeaker) return;
        setAssigningSegmentId(null);
        setAssigningChoice(null);
        setNewSpeakerName('');
    }, [isAssigningSpeaker]);

    const handleAssignExistingSpeaker = useCallback(async (value: string) => {
        const segmentId = assigningSegmentId;
        const speakerId = Number(value);
        if (!segmentId || !Number.isInteger(speakerId) || !onAssignSegmentSpeaker || isAssigningSpeaker) {
            return;
        }

        setIsAssigningSpeaker(true);
        try {
            await onAssignSegmentSpeaker(segmentId, speakerId);
            setAssigningSegmentId(null);
            setAssigningChoice(null);
            setNewSpeakerName('');
        } catch (error) {
            toast.error(
                typeof error === 'string'
                    ? error
                    : (error as any)?.message || t('Failed to save the speaker')
            );
        } finally {
            setIsAssigningSpeaker(false);
        }
    }, [assigningSegmentId, isAssigningSpeaker, onAssignSegmentSpeaker, t]);

    const handleAddNewSpeaker = useCallback(async () => {
        const segmentId = assigningSegmentId;
        const name = newSpeakerName.trim();
        if (!segmentId || !name || !onAddAndAssignSegmentSpeaker || isAssigningSpeaker) return;

        setIsAssigningSpeaker(true);
        try {
            await onAddAndAssignSegmentSpeaker(segmentId, name);
            setAssigningSegmentId(null);
            setAssigningChoice(null);
            setNewSpeakerName('');
        } catch (error) {
            toast.error(
                typeof error === 'string'
                    ? error
                    : (error as any)?.message || t('Failed to save the speaker')
            );
        } finally {
            setIsAssigningSpeaker(false);
        }
    }, [assigningSegmentId, isAssigningSpeaker, newSpeakerName, onAddAndAssignSegmentSpeaker, t]);

    // `playbackTime` advances on every `timeupdate` (~4 Hz). Feeding it to each row would
    // rebuild the whole list four times a second for a highlight that moves once per
    // segment, so collapse it to the set of ids under the playhead and keep that value
    // referentially stable until the highlight actually moves. Overlapping mic/system
    // segments can both be under the playhead, hence a set rather than a single id.
    const activePlaybackKey = useMemo(() => {
        if (playbackTime == null) return '';
        let key = '';
        for (const segment of segments) {
            if (isPlaybackSegmentActive(segment, playbackTime)) key += `${segment.id}\n`;
        }
        return key;
    }, [segments, playbackTime]);
    const activePlaybackIds = useMemo(
        () => new Set(activePlaybackKey ? activePlaybackKey.split('\n').slice(0, -1) : []),
        [activePlaybackKey]
    );

    // Stable so memoized segments don't re-render on every parent render.
    const handleSpeakerClick = useCallback((speakerId: number) => {
        setRenamingSpeaker({
            id: speakerId,
            name: speakersById?.get(speakerId) ?? '',
        });
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
    const reactionsBySegmentId = useTranscriptReactions(segments, streamingSegmentId);

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
        }, 2650);
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
        <ScrollEdgesObserver onChange={onScrollEdgesChange} />
        <MessageScroller className="h-full">
        <MessageScrollerViewport className={cn("px-4 py-2", viewportClassName)}>
        <MessageScrollerContent className="gap-0">
            <div
                className={cn(
                    segments.length === 0 && !isRecording && !showRollCallTip
                        ? "flex min-h-full items-center justify-center"
                        : undefined,
                )}
            >
            {showRollCallTip && (
                <MessageScrollerItem messageId="memento-roll-call-tip" className="w-full">
                    <RollCallTip onDismiss={dismissRollCallTip} />
                </MessageScrollerItem>
            )}
            {segments.length === 0 ? (
                isRecording || showRollCallTip ? null : (
                    <MessageScrollerItem messageId="transcript-empty" className="w-full">
                        <div className="flex items-center justify-center px-6 text-center text-sm text-muted-foreground">
                            {t('No speech was recognized in this recording')}
                        </div>
                    </MessageScrollerItem>
                )
            ) : (
                <>
                    <div className="space-y-1" role="list">
                        {segments.map((segment) => {
                            const isStreaming = streamingSegmentId === segment.id;
                            // Audio source is not identity: an offline meeting can contain
                            // several people on the same microphone. Only a diarized voice
                            // profile explicitly confirmed by the user receives "You" styling.
                            const isOwnSegment = segment.speaker_id != null
                                && (selfSpeakerIds?.has(segment.speaker_id) ?? false);
                            const speakerLabel = isOwnSegment
                                ? t('You')
                                : localizeSpeakerLabel(resolveSpeakerLabel(segment, speakersById), t);

                            return (
                                <MessageScrollerItem
                                    key={segment.id}
                                    messageId={segment.id}
                                    scrollAnchor={isRecording && segment.id === segments.at(-1)?.id}
                                >
                                  {/* A CSS entry animation instead of a per-row motion
                                      component: framer-motion allocates an animation
                                      controller per instance, and a saved transcript
                                      mounts every row at once. Only the live view gains
                                      anything from animating arrivals. */}
                                  <div className={isRecording ? 'animate-in fade-in slide-in-from-bottom-1 duration-150' : undefined}>
                                    <TranscriptSegment
                                        id={segment.id}
                                        text={getDisplayText(segment)}
                                        recognizedAt={segment.recognizedAt}
                                        isStreaming={isStreaming}
                                        highlight={highlightedId === segment.id}
                                        speakerLabel={speakerLabel}
                                        speakerId={segment.speaker_id}
                                        speakerRenamable={
                                            !!onRenameSpeaker &&
                                            segment.speaker_id != null &&
                                            !!speakersById?.has(segment.speaker_id)
                                        }
                                        onSpeakerClick={handleSpeakerClick}
                                        onAssignClick={
                                            (
                                                !!onAddAndAssignSegmentSpeaker
                                                || (!!onAssignSegmentSpeaker && (speakersById?.size ?? 0) > 0)
                                            )
                                            && segment.speaker_id == null
                                                ? () => {
                                                    setAssigningSegmentId(segment.id);
                                                    setAssigningChoice(null);
                                                    setNewSpeakerName('');
                                                }
                                                : undefined
                                        }
                                        playbackActive={activePlaybackIds.has(segment.id)}
                                        isOwn={isOwnSegment}
                                        reaction={reactionsBySegmentId.get(segment.id)}
                                    />
                                  </div>
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

            {/* A line diarization refused, handed to the person who was in the room. */}
            <ShapeProvider defaultShape="rounded" publishToRoot={false}>
                <Dialog
                    open={assigningSegmentId != null}
                    onOpenChange={(open) => { if (!open) closeAssignDialog(); }}
                >
                    <DialogContent className="sm:max-w-[420px]">
                        <DialogHeader>
                            <DialogTitle>{t('Who said this?')}</DialogTitle>
                        </DialogHeader>
                        <div className="flex flex-col gap-3">
                            <RadioGroup
                                value={assigningChoice ?? ''}
                                onValueChange={(value) => {
                                    setAssigningChoice(value);
                                    if (value !== ADD_NEW_SPEAKER_VALUE) {
                                        void handleAssignExistingSpeaker(value);
                                    }
                                }}
                                className="w-full"
                            >
                                {[...(speakersById?.entries() ?? [])].map(([id, name], index) => (
                                    <RadioItem
                                        key={id}
                                        index={index}
                                        value={String(id)}
                                        label={localizeSpeakerLabel(name, t) ?? name}
                                        className="w-full"
                                    />
                                ))}
                                {onAddAndAssignSegmentSpeaker && (
                                    <RadioItem
                                        index={speakersById?.size ?? 0}
                                        value={ADD_NEW_SPEAKER_VALUE}
                                        label={t('Add another person')}
                                        className="w-full"
                                    />
                                )}
                            </RadioGroup>

                            {assigningChoice === ADD_NEW_SPEAKER_VALUE && onAddAndAssignSegmentSpeaker && (
                                <form
                                    className="flex items-center gap-2"
                                    onSubmit={(event) => {
                                        event.preventDefault();
                                        void handleAddNewSpeaker();
                                    }}
                                >
                                    <FluidInput
                                        autoFocus
                                        value={newSpeakerName}
                                        onChange={(event) => setNewSpeakerName(event.target.value)}
                                        placeholder={t('New person name')}
                                        disabled={isAssigningSpeaker}
                                    />
                                    <FluidButton
                                        type="submit"
                                        size="sm"
                                        loading={isAssigningSpeaker}
                                        disabled={!newSpeakerName.trim()}
                                    >
                                        {t('Add person')}
                                    </FluidButton>
                                </form>
                            )}
                        </div>
                    </DialogContent>
                </Dialog>
            </ShapeProvider>

            {/* Rename affordance for diarized speakers (saved meetings only). */}
            {onRenameSpeaker && renamingSpeaker && (
                <SpeakerRenameDialog
                    open={true}
                    currentName={renamingSpeaker.name}
                    onOpenChange={(o) => { if (!o) setRenamingSpeaker(null); }}
                    onRename={(name) => onRenameSpeaker(renamingSpeaker.id, name)}
                    isSelf={selfSpeakerIds?.has(renamingSpeaker.id) ?? false}
                    onSetSelf={
                        onSetSelfSpeaker
                            ? (isSelf) => onSetSelfSpeaker(renamingSpeaker.id, isSelf)
                            : undefined
                    }
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
