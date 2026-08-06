'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { translate, useT } from '@/lib/i18n';
import { prefetchMeetingSummary } from '@/lib/meetingSummaryCache';


interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

export interface CurrentMeeting {
  id: string;
  title: string;
  createdAt?: string | null;
  occurredAt?: string | null;
  folderPath?: string | null;
  durationSeconds?: number | null;
}

// Search result type for transcript search
interface TranscriptSearchResult {
  id: string;
  title: string;
  matchContext: string;
  timestamp: string;
};

// Expanded-sidebar width (px) — user-resizable via the right-edge drag handle,
// persisted across sessions. Consumed by the sidebar itself, MainContent's
// margin, and the home page's record-button offset.
export const SIDEBAR_DEFAULT_WIDTH = 232;
export const SIDEBAR_MIN_WIDTH = 200;
export const SIDEBAR_MAX_WIDTH = 420;
const SIDEBAR_WIDTH_KEY = 'memento:sidebar:width';

function readStoredSidebarWidth(): number {
  if (typeof window === 'undefined') return SIDEBAR_DEFAULT_WIDTH;
  try {
    const parsed = Number(window.localStorage.getItem(SIDEBAR_WIDTH_KEY));
    return Number.isFinite(parsed) && parsed >= SIDEBAR_MIN_WIDTH && parsed <= SIDEBAR_MAX_WIDTH
      ? parsed
      : SIDEBAR_DEFAULT_WIDTH;
  } catch {
    return SIDEBAR_DEFAULT_WIDTH;
  }
}

export function persistSidebarWidth(width: number): void {
  try {
    window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(Math.round(width)));
  } catch { /* ignore */ }
}

export function clearStoredSidebarWidth(): void {
  try {
    window.localStorage.removeItem(SIDEBAR_WIDTH_KEY);
  } catch { /* ignore */ }
}

interface SidebarContextType {
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: React.Dispatch<React.SetStateAction<CurrentMeeting | null>>;
  sidebarItems: SidebarItem[];
  isCollapsed: boolean;
  toggleCollapse: () => void;
  sidebarWidth: number;
  setSidebarWidth: (width: number) => void;
  /** True while the user drags the sidebar handle — consumers disable width transitions. */
  isSidebarResizing: boolean;
  setIsSidebarResizing: (resizing: boolean) => void;
  meetings: CurrentMeeting[];
  setMeetings: React.Dispatch<React.SetStateAction<CurrentMeeting[]>>;
  isMeetingActive: boolean;
  setIsMeetingActive: (active: boolean) => void;
  handleRecordingToggle: () => void;
  searchTranscripts: (query: string) => Promise<void>;
  searchResults: TranscriptSearchResult[];
  isSearching: boolean;
  setServerAddress: (address: string) => void;
  serverAddress: string;
  transcriptServerAddress: string;
  setTranscriptServerAddress: (address: string) => void;
  // Summary polling management. `activeSummaryPolls` is a live map for inspection only — it is
  // mutated in place and does not re-render consumers.
  activeSummaryPolls: Map<string, NodeJS.Timeout>;
  startSummaryPolling: (meetingId: string, processId: string, onUpdate: (result: any) => void) => void;
  stopSummaryPolling: (meetingId: string) => void;
  // Refetch meetings from backend
  refetchMeetings: () => Promise<void>;

}

const SidebarContext = createContext<SidebarContextType | null>(null);

export const useSidebar = () => {
  const context = useContext(SidebarContext);
  if (!context) {
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return context;
};

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const t = useT();
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>({ id: 'intro-call', title: t('+ New Call') });
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState<number>(readStoredSidebarWidth);
  const [isSidebarResizing, setIsSidebarResizing] = useState(false);
  const [meetings, setMeetings] = useState<CurrentMeeting[]>([]);
  const [sidebarItems, setSidebarItems] = useState<SidebarItem[]>([]);
  const [isMeetingActive, setIsMeetingActive] = useState(false);
  const [searchResults, setSearchResults] = useState<any[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [serverAddress, setServerAddress] = useState('');
  const [transcriptServerAddress, setTranscriptServerAddress] = useState('');
  // Live poll timers live in a ref, never in state: a re-render must not be able to cancel a
  // generation that is still running. Keying the lifecycle off a state Map used to hand the
  // cleanup effect a stale snapshot and clear intervals belonging to other meetings.
  const activeSummaryPollsRef = React.useRef<Map<string, NodeJS.Timeout>>(new Map());
  const latestSearchRequestRef = React.useRef(0);
  const meetingsRequestRef = React.useRef<Promise<boolean> | null>(null);

  // Use recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();

  const pathname = usePathname();
  const router = useRouter();

  // Extract fetchMeetings as a reusable function
  const fetchMeetings = React.useCallback((): Promise<boolean> => {
    if (meetingsRequestRef.current) return meetingsRequestRef.current;

    const request = (async () => {
      try {
        const meetings = await invoke('api_get_meetings', { authToken: null }) as Array<{
          id: string;
          title: string;
          created_at: string;
          occurred_at?: string | null;
          folder_path?: string | null;
          duration_seconds?: number | null;
        }>;
        const transformedMeetings = meetings.map((meeting) => ({
          id: meeting.id,
          title: meeting.title,
          createdAt: meeting.created_at,
          occurredAt: meeting.occurred_at ?? null,
          folderPath: meeting.folder_path ?? null,
          durationSeconds: meeting.duration_seconds ?? null,
        }));
        setMeetings((previousMeetings) => {
          // Route drawers render the archive in the background and the home
          // route renders it again after the drawer closes. Keep the existing
          // array when the backend returned the same records so that this
          // hand-off does not trigger a second layout pass.
          if (
            previousMeetings.length === transformedMeetings.length
            && previousMeetings.every((previous, index) => {
              const next = transformedMeetings[index];
              return previous.id === next.id
                && previous.title === next.title
                && previous.createdAt === next.createdAt
                && previous.occurredAt === next.occurredAt
                && previous.folderPath === next.folderPath
                && previous.durationSeconds === next.durationSeconds;
            })
          ) {
            return previousMeetings;
          }

          return transformedMeetings;
        });
        // Warm the small in-memory summary cache as soon as the archive becomes
        // available. Reads are independent SQLite calls and are de-duplicated by
        // meetingSummaryCache, so opening a drawer can render saved content at once.
        void Promise.allSettled(
          transformedMeetings.map((meeting) => prefetchMeetingSummary(meeting.id)),
        );
        Analytics.trackBackendConnection(true);
        return true;
      } catch (error) {
        console.error('Error fetching meetings:', error);
        Analytics.trackBackendConnection(false, error instanceof Error ? error.message : 'Unknown error');
        return false;
      }
    })();

    meetingsRequestRef.current = request;
    void request.then(() => {
      if (meetingsRequestRef.current === request) meetingsRequestRef.current = null;
    });
    return request;
  }, []);

  const refetchMeetings = React.useCallback(async (): Promise<void> => {
    await fetchMeetings();
  }, [fetchMeetings]);

  useEffect(() => {
    let cancelled = false;
    let retryTimer: number | undefined;
    let attempts = 0;

    // A freshly opened WebView can hydrate before Tauri IPC and the database
    // are ready. Retry a failed read instead of leaving the archive empty for
    // the whole session after that startup race.
    const loadMeetings = async () => {
      const loaded = await fetchMeetings();
      attempts += 1;
      if (!cancelled && !loaded && attempts < 5) {
        retryTimer = window.setTimeout(() => void loadMeetings(), attempts * 1_000);
      }
    };

    void loadMeetings();
    const handleFocus = () => void fetchMeetings();
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') void fetchMeetings();
    };
    window.addEventListener('focus', handleFocus);
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      cancelled = true;
      if (retryTimer) window.clearTimeout(retryTimer);
      window.removeEventListener('focus', handleFocus);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [fetchMeetings]);

  useEffect(() => {
    const fetchSettings = async () => {
      setServerAddress('http://localhost:5167');
      setTranscriptServerAddress('http://127.0.0.1:8178/stream');
    };
    fetchSettings();
  }, []);

  const baseItems: SidebarItem[] = [
    {
      id: 'meetings',
      title: t('Meeting Notes'),
      type: 'folder' as const,
      children: [
        ...meetings.map(meeting => ({
          id: meeting.id,
          title: meeting.title,
          createdAt: meeting.createdAt,
          occurredAt: meeting.occurredAt,
          folderPath: meeting.folderPath,
          type: 'file' as const,
        }))
      ]
    },
  ];


  const toggleCollapse = () => {
    setIsCollapsed(!isCollapsed);
  };

  // Update current meeting when on home page
  useEffect(() => {
    if (pathname === '/') {
      setCurrentMeeting({ id: 'intro-call', title: t('+ New Call') });
    }
    setSidebarItems(baseItems);
  }, [pathname]);

  // Update sidebar items when meetings change
  useEffect(() => {
    setSidebarItems(baseItems);
  }, [meetings]);

  // Function to handle recording toggle from sidebar
  const handleRecordingToggle = () => {
    if (!isRecording) {
      // Check if already on home page
      if (pathname === '/') {
        // Already on home - trigger recording directly via custom event
        console.log('Triggering recording from sidebar (already on home page)');
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else {
        // Not on home - navigate and use auto-start mechanism
        console.log('Navigating to home page with auto-start flag');
        sessionStorage.setItem('autoStartRecording', 'true');
        router.push('/');
      }

      // Track recording initiation from sidebar
      Analytics.trackButtonClick('start_recording', 'sidebar');
    }
    // The actual recording start/stop is handled in the Home component
  };

  // Function to search through meeting transcripts
  const searchTranscripts = React.useCallback(async (query: string) => {
    const requestId = ++latestSearchRequestRef.current;
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }

    try {
      setIsSearching(true);


      const results = await invoke('api_search_transcripts', { query }) as TranscriptSearchResult[];
      if (requestId === latestSearchRequestRef.current) {
        setSearchResults(results);
      }
    } catch (error) {
      console.error('Error searching transcripts:', error);
      if (requestId === latestSearchRequestRef.current) {
        setSearchResults([]);
      }
    } finally {
      if (requestId === latestSearchRequestRef.current) {
        setIsSearching(false);
      }
    }
  }, []);

  // Summary polling management. Both callbacks are referentially stable: consumers keep them
  // in effect dependency lists, and an identity change there would tear down a running poll.
  const startSummaryPolling = React.useCallback((
    meetingId: string,
    processId: string,
    onUpdate: (result: any) => void
  ) => {
    const polls = activeSummaryPollsRef.current;

    // Stop existing poll for this meeting if any
    const previousInterval = polls.get(meetingId);
    if (previousInterval) {
      clearInterval(previousInterval);
      polls.delete(meetingId);
    }

    console.log(`📊 Starting polling for meeting ${meetingId}, process ${processId}`);

    let pollCount = 0;
    const MAX_POLLS = 200; // ~16.5 minutes at 5-second intervals (slightly longer than backend's 15-min timeout to avoid race conditions)

    const finish = () => {
      clearInterval(pollInterval);
      if (polls.get(meetingId) === pollInterval) polls.delete(meetingId);
    };

    const pollInterval = setInterval(async () => {
      pollCount++;

      // Timeout safety: Stop after 10 minutes
      if (pollCount >= MAX_POLLS) {
        console.warn(`⏱️ Polling timeout for ${meetingId} after ${MAX_POLLS} iterations`);
        finish();
        onUpdate({
          status: 'error',
          error: translate('Summary generation timed out after 15 minutes. Please try again or check your model configuration.')
        });
        return;
      }
      try {
        const result = await invoke('api_get_summary', {
          meetingId: meetingId,
        }) as any;

        console.log(`📊 Polling update for ${meetingId}:`, result.status);

        // Stop polling if completed, error, failed, cancelled, or idle (after initial
        // processing). `pollEnded` is handed to the callback so it can always settle its own
        // status instead of leaving a progress indicator running after the last update.
        const isTerminal = result.status === 'completed'
          || result.status === 'error'
          || result.status === 'failed'
          || result.status === 'cancelled'
          // 'idle' after polling started means the process row completed/disappeared.
          || (result.status === 'idle' && pollCount > 1);
        if (isTerminal) {
          console.log(`Polling finished for ${meetingId}, status: ${result.status}`);
          finish();
        }

        onUpdate({ ...result, pollEnded: isTerminal });
      } catch (error) {
        console.error(`Polling error for ${meetingId}:`, error);
        finish();
        // Report error to callback
        onUpdate({
          status: 'error',
          pollEnded: true,
          error: error instanceof Error ? error.message : translate('Unknown error')
        });
      }
    }, 5000); // Poll every 5 seconds

    polls.set(meetingId, pollInterval);
  }, []);

  const stopSummaryPolling = React.useCallback((meetingId: string) => {
    const pollInterval = activeSummaryPollsRef.current.get(meetingId);
    if (pollInterval) {
      console.log(`⏹️ Stopping polling for meeting ${meetingId}`);
      clearInterval(pollInterval);
      activeSummaryPollsRef.current.delete(meetingId);
    }
  }, []);

  // Cleanup all polling intervals on unmount
  useEffect(() => {
    const polls = activeSummaryPollsRef.current;
    return () => {
      console.log('🧹 Cleaning up all summary polling intervals');
      polls.forEach(interval => clearInterval(interval));
      polls.clear();
    };
  }, []);



  return (
    <SidebarContext.Provider value={{
      currentMeeting,
      setCurrentMeeting,
      sidebarItems,
      isCollapsed,
      toggleCollapse,
      sidebarWidth,
      setSidebarWidth,
      isSidebarResizing,
      setIsSidebarResizing,
      meetings,
      setMeetings,
      isMeetingActive,
      setIsMeetingActive,
      handleRecordingToggle,
      searchTranscripts,
      searchResults,
      isSearching,
      setServerAddress,
      serverAddress,
      transcriptServerAddress,
      setTranscriptServerAddress,
      activeSummaryPolls: activeSummaryPollsRef.current,
      startSummaryPolling,
      stopSummaryPolling,
      refetchMeetings,

    }}>
      {children}
    </SidebarContext.Provider>
  );
}
