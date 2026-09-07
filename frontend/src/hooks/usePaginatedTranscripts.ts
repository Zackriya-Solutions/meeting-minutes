import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Transcript, MeetingMetadata, PaginatedTranscriptsResponse, TranscriptSegmentData } from "@/types";

const DEFAULT_PAGE_SIZE = 100;

interface UsePaginatedTranscriptsProps {
    meetingId: string | null;
    /** Optional initial timestamp (in seconds) from URL for loading the correct page */
    initialTimestamp?: number;
}

interface UsePaginatedTranscriptsReturn {
    metadata: MeetingMetadata | null;
    segments: TranscriptSegmentData[];
    transcripts: Transcript[];
    isLoading: boolean;
    isLoadingMore: boolean;
    hasMore: boolean;
    totalCount: number;
    loadedCount: number;
    error: string | null;

    // Actions
    loadMore: () => Promise<void>;
    reset: () => void;
    refetch: () => Promise<void>;
}

/**
 * Convert Transcript array to TranscriptSegmentData for virtualized display
 */
function convertTranscriptsToSegments(transcripts: Transcript[]): TranscriptSegmentData[] {
    return transcripts.map(t => ({
        id: t.id,
        timestamp: t.audio_start_time ?? 0,
        endTime: t.audio_end_time,
        text: t.text,
        confidence: t.confidence,
    }));
}

export function usePaginatedTranscripts({
    meetingId,
    initialTimestamp,
}: UsePaginatedTranscriptsProps): UsePaginatedTranscriptsReturn {
    const [metadata, setMetadata] = useState<MeetingMetadata | null>(null);
    const [transcripts, setTranscripts] = useState<Transcript[]>([]);
    const [totalCount, setTotalCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [isLoadingMore, setIsLoadingMore] = useState(false);
    const [hasMore, setHasMore] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const offsetRef = useRef(0);
    const activeMeetingIdRef = useRef<string | null>(null);
    const requestIdRef = useRef(0);
    const isLoadingRef = useRef(false);
    const lastLoadTimeRef = useRef(0); // Debounce protection

    const isCurrentRequest = useCallback((requestId: number) =>
        requestIdRef.current === requestId && activeMeetingIdRef.current === meetingId,
        [meetingId]
    );

    // Reset invalidates pending reads, including same-meeting refetches.
    const reset = useCallback(() => {
        requestIdRef.current += 1;
        isLoadingRef.current = false;
        lastLoadTimeRef.current = 0;
        setMetadata(null);
        setTranscripts([]);
        setTotalCount(0);
        setIsLoading(true);
        setIsLoadingMore(false);
        setHasMore(false);
        setError(null);
        offsetRef.current = 0;
    }, []);

    // Load meeting metadata
    const loadMetadata = useCallback(async (requestId: number): Promise<MeetingMetadata | null> => {
        if (!meetingId || !isCurrentRequest(requestId)) return null;

        try {
            const data = await invoke<MeetingMetadata>('api_get_meeting_metadata', {
                meetingId,
            });
            if (!isCurrentRequest(requestId)) return null;
            setMetadata(data);
            return data;
        } catch (err) {
            if (!isCurrentRequest(requestId)) return null;
            console.error('Failed to load meeting metadata:', err);
            setError('Failed to load meeting details');
            return null;
        }
    }, [meetingId, isCurrentRequest]);

    // Load transcripts at specific offset
    const loadTranscriptsAtOffset = useCallback(async (
        requestId: number,
        offset: number,
        append: boolean = true
    ): Promise<Transcript[]> => {
        if (!meetingId || !isCurrentRequest(requestId)) return [];

        try {
            const response = await invoke<PaginatedTranscriptsResponse>(
                'api_get_meeting_transcripts',
                {
                    meetingId,
                    limit: DEFAULT_PAGE_SIZE,
                    offset,
                }
            );

            if (!isCurrentRequest(requestId)) return [];
            const newTranscripts = response.transcripts;

            if (append) {
                setTranscripts(prev => {
                    if (!isCurrentRequest(requestId)) return prev;
                    // Deduplicate by id
                    const existingIds = new Set(prev.map(t => t.id));
                    const uniqueNew = newTranscripts.filter(t => !existingIds.has(t.id));
                    // Sort by audio_start_time
                    return [...prev, ...uniqueNew].sort((a, b) =>
                        (a.audio_start_time ?? 0) - (b.audio_start_time ?? 0)
                    );
                });
            } else {
                setTranscripts(newTranscripts);
            }

            setHasMore(response.has_more);
            setTotalCount(response.total_count);
            offsetRef.current = offset + newTranscripts.length;

            return newTranscripts;
        } catch (err) {
            if (!isCurrentRequest(requestId)) return [];
            console.error('Failed to load transcripts:', err);
            setError('Failed to load transcripts');
            return [];
        }
    }, [meetingId, isCurrentRequest]);

    // Load next page with debounce protection
    const loadMore = useCallback(async () => {
        const requestId = requestIdRef.current;
        if (!isCurrentRequest(requestId)) return;
        const now = Date.now();
        // Debounce: require at least 100ms between calls
        if (now - lastLoadTimeRef.current < 100) {
            return;
        }

        if (isLoadingRef.current || !hasMore || !meetingId || isLoading) return;

        lastLoadTimeRef.current = now;
        isLoadingRef.current = true;
        setIsLoadingMore(true);
        try {
            await loadTranscriptsAtOffset(requestId, offsetRef.current, true);
        } finally {
            if (isCurrentRequest(requestId)) {
                setIsLoadingMore(false);
                isLoadingRef.current = false;
            }
        }
    }, [hasMore, meetingId, loadTranscriptsAtOffset, isLoading, isCurrentRequest]);

    // Force refetch of data (e.g., after retranscription)
    const refetch = useCallback(async () => {
        if (!meetingId || activeMeetingIdRef.current !== meetingId) return;

        reset();
        const requestId = requestIdRef.current;
        try {
            await loadMetadata(requestId);
            await loadTranscriptsAtOffset(requestId, 0, false);
        } finally {
            if (isCurrentRequest(requestId)) setIsLoading(false);
        }
    }, [meetingId, reset, loadMetadata, loadTranscriptsAtOffset, isCurrentRequest]);

    // A new meeting or effect lifetime owns its own requests.
    useEffect(() => {
        activeMeetingIdRef.current = meetingId;
        if (meetingId) {
            void refetch();
        } else {
            reset();
        }

        return () => {
            requestIdRef.current += 1;
            activeMeetingIdRef.current = null;
        };
    }, [meetingId, reset, refetch]);

    // Convert to segments (memoized)
    const segments = useMemo(() =>
        convertTranscriptsToSegments(transcripts),
        [transcripts]
    );

    return {
        metadata,
        segments,
        transcripts,
        isLoading,
        isLoadingMore,
        hasMore,
        totalCount,
        loadedCount: transcripts.length,
        error,
        loadMore,
        reset,
        refetch,
    };
}
