"use client"
import { useSidebar } from "@/components/Sidebar/SidebarProvider";
import { useState, useEffect, useCallback, useRef, Suspense } from "react";
import { Transcript, Summary } from "@/types";
import PageContent from "./page-content";
import { useSearchParams } from "next/navigation";
import Analytics from "@/lib/analytics";
import { invoke } from "@tauri-apps/api/core";
import { FluidSpinner } from '@/components/ui/fluid-spinner';
import { usePaginatedTranscripts } from "@/hooks/usePaginatedTranscripts";
import { useMeetingSpeakers } from "@/hooks/useMeetingSpeakers";
import { useLanguage, useT } from "@/lib/i18n";
import { useMeetingDrawer } from "@/contexts/MeetingDrawerContext";
import { MeetingDrawerShell } from "./meeting-drawer-shell";
import {
  cacheMeetingSummary,
  parsePersistedSummary,
  readCachedMeetingSummary,
} from '@/lib/meetingSummaryCache';

interface MeetingDetailsResponse {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  occurred_at?: string | null;
  transcripts: Transcript[];
  folder_path?: string;
  duration_seconds?: number | null;
}

type SummaryLoadStatus = 'loading' | 'loaded' | 'absent' | 'error';

function UpcomingMeetingPreview() {
  const searchParams = useSearchParams();
  const { t, lang } = useLanguage();
  const title = searchParams.get('title') || t('Upcoming meeting');
  const start = new Date(searchParams.get('start') || '');
  const end = new Date(searchParams.get('end') || '');
  const hasStart = !Number.isNaN(start.getTime());
  const hasEnd = !Number.isNaN(end.getTime());
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  const dateLabel = hasStart
    ? new Intl.DateTimeFormat(locale, {
        weekday: 'long',
        day: 'numeric',
        month: 'long',
      }).format(start)
    : '';
  const timeFormatter = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });
  const timeLabel = hasStart
    ? `${timeFormatter.format(start)}${hasEnd ? `\u00a0–\u00a0${timeFormatter.format(end)}` : ''}`
    : '';

  return (
    <div className="meeting-page-surface flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-border px-[var(--drawer-content-inset)] py-4">
        <div className="min-w-0 flex-1">
          <h1 className="memento-screen-title truncate text-foreground">{title}</h1>
          {(dateLabel || timeLabel) && (
            <p className="mm-numeric mt-0.5 truncate text-xs text-muted-foreground">
              {[dateLabel, timeLabel].filter(Boolean).join(' · ')}
            </p>
          )}
        </div>
      </div>
      <main className="flex min-h-0 flex-1 items-center justify-center px-[var(--drawer-content-inset)] py-12">
        <div className="max-w-md text-center">
          <p className="text-base font-medium text-foreground">{t('This meeting has not started yet')}</p>
          <p className="mt-2 text-sm leading-relaxed text-[var(--primary-40)]">
            {t('The transcript and summary will appear here after the meeting is recorded.')}
          </p>
        </div>
      </main>
    </div>
  );
}

function MeetingDetailsContent() {
  const searchParams = useSearchParams();
  const meetingId = searchParams.get('id');
  // Optional jump-to-timestamp deep link (?t=<seconds>) from search results / RAG citations.
  const seekParam = searchParams.get('t');
  const seekToSeconds =
    seekParam != null && seekParam !== '' && Number.isFinite(Number(seekParam)) ? Number(seekParam) : null;
  const { setCurrentMeeting, refetchMeetings, stopSummaryPolling } = useSidebar();
  const meetingDrawer = useMeetingDrawer();
  const t = useT();
  const [meetingDetails, setMeetingDetails] = useState<MeetingDetailsResponse | null>(null);
  const initialCachedSummary = meetingId ? readCachedMeetingSummary(meetingId) : null;
  const [meetingSummary, setMeetingSummary] = useState<Summary | null>(initialCachedSummary);
  const meetingSummaryRef = useRef<Summary | null>(initialCachedSummary);
  const summaryLoadRequestRef = useRef(0);
  const [summaryLoadStatus, setSummaryLoadStatus] = useState<SummaryLoadStatus>(
    initialCachedSummary ? 'loaded' : 'loading',
  );
  const [summaryLoadError, setSummaryLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);

  // Use pagination hook for efficient transcript loading
  const {
    metadata,
    segments,
    transcripts,
    isLoading: isLoadingTranscripts,
    isLoadingMore,
    hasMore,
    totalCount,
    loadedCount,
    loadMore,
    refetch,
    error: transcriptError,
  } = usePaginatedTranscripts({ meetingId: meetingId || '' });

  // Diarized speaker identities for this meeting. Kept here (alongside the
  // transcript pagination hook) so the `diarization-complete` listener can
  // refresh both speakers and transcripts for the currently open meeting.
  const {
    speakers,
    speakersById,
    selfSpeakerIds,
    refetchSpeakers,
    renameSpeaker,
    setSelfSpeaker,
  } = useMeetingSpeakers({
    meetingId: meetingId || null,
    onDiarized: refetch,
  });

  const commitMeetingSummary = useCallback((summary: Summary | null) => {
    meetingSummaryRef.current = summary;
    setMeetingSummary(summary);
    setSummaryLoadStatus(summary ? 'loaded' : 'absent');
    setSummaryLoadError(null);
    if (meetingId) cacheMeetingSummary(meetingId, summary);
  }, [meetingId]);

  // Explicit refresh after a Detect action completes (immediate feedback;
  // the diarization-complete event covers refreshes triggered elsewhere).
  const handleSpeakersDetected = useCallback(async () => {
    await refetchSpeakers();
    await refetch();
  }, [refetchSpeakers, refetch]);

  const handleRenameSpeaker = useCallback(async (speakerId: number, displayName: string) => {
    await renameSpeaker(speakerId, displayName);
    // A persisted summary is editable prose, not a live view over speaker ids. Mark it stale
    // immediately; api_get_summary independently verifies the persisted generation snapshot on
    // the next load. We deliberately do not rewrite or regenerate over manual edits silently.
    setMeetingSummary((previous) => {
      if (!previous) return previous;
      const next = {
        ...(previous as any),
        summary_freshness: {
          ...((previous as any).summary_freshness || {}),
          speaker_attribution_status: 'stale',
          speaker_attribution_stale: true,
        },
      } as any;
      meetingSummaryRef.current = next;
      return next;
    });
  }, [renameSpeaker]);

  const handleSetSelfSpeaker = useCallback(async (speakerId: number, isSelf: boolean) => {
    await setSelfSpeaker(speakerId, isSelf);
    // Owner identity changes the labels supplied to future summaries just like a rename.
    setMeetingSummary((previous) => {
      if (!previous) return previous;
      const next = {
        ...(previous as any),
        summary_freshness: {
          ...((previous as any).summary_freshness || {}),
          speaker_attribution_status: 'stale',
          speaker_attribution_stale: true,
        },
      } as any;
      meetingSummaryRef.current = next;
      return next;
    });
  }, [setSelfSpeaker]);

  // Sync meeting metadata from pagination hook to meeting details state
  useEffect(() => {
    if (metadata && (!meetingId || meetingId === 'intro-call')) {
      // If invalid meeting ID, don't sync
      return;
    }

    if (metadata) {
      console.log('Meeting metadata loaded:', metadata);

      // Build meeting details from metadata and paginated transcripts
      setMeetingDetails({
        id: metadata.id,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        occurred_at: metadata.occurred_at,
        transcripts: transcripts, // Paginated transcripts from hook
        folder_path: metadata.folder_path, // For retranscription feature
        duration_seconds: metadata.duration_seconds,
      });

      // Sync with sidebar context
      setCurrentMeeting({
        id: metadata.id,
        title: metadata.title,
        createdAt: metadata.created_at,
        occurredAt: metadata.occurred_at,
        folderPath: metadata.folder_path,
        durationSeconds: metadata.duration_seconds,
      });
    }
  }, [metadata, transcripts, meetingId, setCurrentMeeting]);

  // Handle transcript loading errors
  useEffect(() => {
    if (transcriptError) {
      console.error('Error loading transcripts:', transcriptError);
      setError(transcriptError);
    }
  }, [transcriptError]);

  // Extract fetchMeetingDetails for use in child components (now refetches via hook)
  const fetchMeetingDetails = useCallback(async () => {
    if (!meetingId || meetingId === 'intro-call') {
      return;
    }

    // The usePaginatedTranscripts hook automatically refetches when meetingId changes
    // This function is kept for compatibility with onMeetingUpdated callback
    console.log('fetchMeetingDetails called - pagination hook will handle refetch');
  }, [meetingId]);

  const fetchMeetingSummary = useCallback(async (showPageLoader = false) => {
    if (!meetingId || meetingId === 'intro-call') return;

    const requestId = ++summaryLoadRequestRef.current;
    if (showPageLoader) setIsLoading(true);
    if (!meetingSummaryRef.current) setSummaryLoadStatus('loading');
    setSummaryLoadError(null);

    let lastError: unknown = null;
    try {
      // SQLite reads should normally be instant, but background migrations/jobs can produce a
      // transient failure. Retry before presenting an error and never replace known content.
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          const response = await invoke('api_get_summary', { meetingId }) as any;
          if (summaryLoadRequestRef.current !== requestId) return;

          if (response?.data != null) {
            const parsed = parsePersistedSummary(response.data);
            if (!parsed) {
              throw new Error(t('The saved summary has an unsupported or damaged format.'));
            }
            commitMeetingSummary(parsed);
            return;
          }

          if (response?.status === 'idle') {
            if (meetingSummaryRef.current) {
              // A late/stale `idle` response must not erase content already shown in this
              // session. Keep it and surface the inconsistency as recoverable load trouble.
              setSummaryLoadStatus('loaded');
              setSummaryLoadError(t('Could not verify the saved summary. The last loaded version is still shown.'));
            } else {
              commitMeetingSummary(null);
            }
            return;
          }

          if (['pending', 'processing', 'summarizing', 'regenerating'].includes(response?.status)) {
            setSummaryLoadStatus(meetingSummaryRef.current ? 'loaded' : 'loading');
            return;
          }

          if (['failed', 'error', 'cancelled'].includes(response?.status)) {
            if (meetingSummaryRef.current) {
              setSummaryLoadStatus('loaded');
            } else {
              // A failed background attempt is still a missing summary. Mark it as
              // absent so the automatic generation effect can retry once for this
              // meeting instead of permanently stranding the drawer in an error
              // snapshot created by an earlier app/network failure.
              setSummaryLoadStatus('absent');
            }
            setSummaryLoadError(response?.error || t('The summary could not be generated automatically.'));
            return;
          }

          throw new Error(response?.error || t('The saved summary could not be loaded.'));
        } catch (error) {
          lastError = error;
          if (attempt < 2) {
            await new Promise((resolve) => window.setTimeout(resolve, 150 * (attempt + 1)));
          }
        }
      }

      if (summaryLoadRequestRef.current !== requestId) return;
      console.error('FETCH SUMMARY: exhausted retries:', lastError);
      setSummaryLoadStatus(meetingSummaryRef.current ? 'loaded' : 'error');
      setSummaryLoadError(
        lastError instanceof Error ? lastError.message : t('The saved summary could not be loaded.')
      );
    } finally {
      if (summaryLoadRequestRef.current === requestId) setIsLoading(false);
    }
  }, [meetingId, commitMeetingSummary, t]);

  // Reset states when meetingId changes (prevent race conditions)
  useEffect(() => {
    const cachedSummary = meetingId ? readCachedMeetingSummary(meetingId) : null;
    setMeetingDetails(null);
    setMeetingSummary(cachedSummary);
    meetingSummaryRef.current = cachedSummary;
    setSummaryLoadStatus(cachedSummary ? 'loaded' : 'loading');
    setSummaryLoadError(null);
    setError(null);
    setIsLoading(true);
  }, [meetingId]);

  // Cleanup: Stop polling when navigating away from a meeting
  useEffect(() => {
    return () => {
      if (meetingId) {
        console.log('Cleaning up: Stopping summary polling for meeting:', meetingId);
        stopSummaryPolling(meetingId);
      }
    };
  }, [meetingId, stopSummaryPolling]);

  useEffect(() => {
    console.log('MeetingDetails useEffect triggered - meetingId:', meetingId);

    if (!meetingId || meetingId === 'intro-call') {
      console.warn('No valid meeting ID in URL - meetingId:', meetingId);
      setError(t('No meeting selected'));
      setIsLoading(false);
      Analytics.trackPageView('meeting_details');
      return;
    }

    console.log('Valid meeting ID found, fetching details for:', meetingId);
    void fetchMeetingSummary(!meetingSummaryRef.current);

    return () => {
      // Ignore a late response from a meeting that is no longer open.
      summaryLoadRequestRef.current += 1;
    };
  }, [meetingId, fetchMeetingSummary, t]);

  // A summary may be queued before this drawer opens (the normal recording-stop
  // path). Keep reading the persisted background job until it resolves instead
  // of leaving the drawer on a permanent "not created"/loading snapshot.
  useEffect(() => {
    if (!meetingId || meetingId === 'intro-call' || summaryLoadStatus !== 'loading') return;

    let cancelled = false;
    let timer: number | null = null;

    const poll = async () => {
      await fetchMeetingSummary(false);
      if (!cancelled) timer = window.setTimeout(poll, 1500);
    };

    timer = window.setTimeout(poll, 750);
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [meetingId, summaryLoadStatus, fetchMeetingSummary]);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <p className="text-destructive mb-4">{error}</p>
          <button
            onClick={() => meetingDrawer?.close()}
            className="px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90"
          >
            {t('Go Back')}
          </button>
        </div>
      </div>
    );
  }

  // Show loading spinner while initial data loads
  if ((isLoading || isLoadingTranscripts) || !meetingDetails) {
    return <div className="flex h-full items-center justify-center">
      <FluidSpinner className="size-6 text-[var(--primary-50)]" />
    </div>;
  }

  return <PageContent
    meeting={meetingDetails}
    summaryData={meetingSummary}
    summaryLoadStatus={summaryLoadStatus}
    summaryLoadError={summaryLoadError}
    onRetrySummary={() => fetchMeetingSummary(false)}
    onSummaryDataChange={commitMeetingSummary}
    shouldAutoGenerate={
      summaryLoadStatus === 'absent'
      && meetingSummary === null
      && ((totalCount ?? 0) > 0 || transcripts.length > 0)
    }
    onMeetingUpdated={async () => {
      // Refetch meeting details to get updated title from backend
      await fetchMeetingDetails();
      // Refetch meetings list to update sidebar
      await refetchMeetings();
    }}
    onRefetchTranscripts={refetch}
    // Pagination props for efficient transcript loading
    segments={segments}
    hasMore={hasMore}
    isLoadingMore={isLoadingMore}
    totalCount={totalCount}
    loadedCount={loadedCount}
    onLoadMore={loadMore}
    seekToSeconds={seekToSeconds}
    // Speaker diarization props
    speakersById={speakersById}
    speakers={speakers}
    selfSpeakerIds={selfSpeakerIds}
    speakerCount={speakers.length}
    onRenameSpeaker={handleRenameSpeaker}
    onSetSelfSpeaker={handleSetSelfSpeaker}
    onSpeakersDetected={handleSpeakersDetected}
  />;
}

export default function MeetingDetails() {
  return (
    <MeetingDrawerShell>
      <Suspense fallback={
        <div className="flex h-full items-center justify-center">
          <FluidSpinner className="size-6 text-[var(--primary-50)]" />
        </div>
      }>
        <MeetingDetailsRoute />
      </Suspense>
    </MeetingDrawerShell>
  );
}

function MeetingDetailsRoute() {
  const searchParams = useSearchParams();
  return searchParams.get('upcoming') === '1'
    ? <UpcomingMeetingPreview />
    : <MeetingDetailsContent />;
}
