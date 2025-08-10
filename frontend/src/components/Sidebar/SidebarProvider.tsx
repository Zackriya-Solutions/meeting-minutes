'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import { load } from '@tauri-apps/plugin-store';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';


interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

export interface CurrentMeeting {
  id: string;
  title: string;
}

// Search result type for transcript search
interface TranscriptSearchResult {
  id: string;
  title: string;
  matchContext: string;
  timestamp: string;
};

interface SidebarContextType {
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: (meeting: CurrentMeeting | null) => void;
  sidebarItems: SidebarItem[];
  isCollapsed: boolean;
  toggleCollapse: () => void;
  meetings: CurrentMeeting[];
  setMeetings: (meetings: CurrentMeeting[]) => void;
  isMeetingActive: boolean;
  setIsMeetingActive: (active: boolean) => void;
  isRecording: boolean;
  setIsRecording: (recording: boolean) => void;
  handleRecordingToggle: () => void;
  searchTranscripts: (query: string) => Promise<void>;
  searchResults: TranscriptSearchResult[];
  isSearching: boolean;
  setServerAddress: (address: string) => void;
  serverAddress: string;
  transcriptServerAddress: string;
  setTranscriptServerAddress: (address: string) => void;
  debugStoreContents: () => Promise<void>;
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
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>({ id: 'intro-call', title: '+ New Call' });
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [meetings, setMeetings] = useState<CurrentMeeting[]>([]);
  const [sidebarItems, setSidebarItems] = useState<SidebarItem[]>([]);
  const [isMeetingActive, setIsMeetingActive] = useState(false);
  const [isRecording, setIsRecording] = useState(false);
  const [searchResults, setSearchResults] = useState<any[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [serverAddress, setServerAddress] = useState('');
  const [transcriptServerAddress, setTranscriptServerAddress] = useState('');


  const pathname = usePathname();
  const router = useRouter();

  useEffect(() => {
    const fetchMeetings = async () => {
        if (serverAddress) {
          try {
        const meetings = await invoke('api_get_meetings') as Array<{id: string, title: string}>;
        const transformedMeetings = meetings.map((meeting: any) => ({
            id: meeting.id,
            title: meeting.title
        }));
            setMeetings(transformedMeetings);
            router.push('/');
            Analytics.trackBackendConnection(true);
          } catch (error) {
            console.error('Error fetching meetings:', error);
            setMeetings([]);
            router.push('/');
            Analytics.trackBackendConnection(false, error instanceof Error ? error.message : 'Unknown error');
          }
        }
    }
    fetchMeetings();
}, [serverAddress]);

  useEffect(() => {
    const fetchSettings = async () => {
        const store = await load('store.json', { autoSave: false });
        let serverAddress = await store.get('appServerUrl') as string | null;
        let transcriptServerAddress = await store.get('transcriptServerUrl') as string | null;
        if (!serverAddress) {
          await store.set('appServerUrl', 'http://localhost:5167');
          serverAddress = await store.get('appServerUrl') as string;
          await store.save();
        }
        if (!transcriptServerAddress) {
          await store.set('transcriptServerUrl', 'http://127.0.0.1:8178/stream');
          transcriptServerAddress = await store.get('transcriptServerUrl') as string;
          await store.save();
        }
        setServerAddress(serverAddress);
        setTranscriptServerAddress(transcriptServerAddress);
        
      
    };
    fetchSettings();
  }, []);

  const baseItems: SidebarItem[] = [
    {
      id: 'meetings',
      title: 'Meeting Notes',
      type: 'folder' as const,
      children: [
        ...meetings.map(meeting => ({ id: meeting.id, title: meeting.title, type: 'file' as const }))
      ]
    },
  ];

 

  const toggleCollapse = () => {
    setIsCollapsed(!isCollapsed);
  };

  // Update current meeting when on home page
  useEffect(() => {
    if (pathname === '/') {
      setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
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
      // If not recording, navigate to home page and set flag to start recording automatically
      sessionStorage.setItem('autoStartRecording', 'true');
      router.push('/');
      
      // Track recording initiation from sidebar
      Analytics.trackButtonClick('start_recording', 'sidebar');
    }
    // The actual recording start/stop is handled in the Home component
  };
  
  // Function to search through meeting transcripts
  const searchTranscripts = async (query: string) => {
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }
    
    try {
      setIsSearching(true);
      
      
      const results = await invoke('api_search_transcripts', { query }) as TranscriptSearchResult[];
      setSearchResults(results);
      
      // Track search performed
      Analytics.trackSearchPerformed(query, results.length);
    } catch (error) {
      console.error('Error searching transcripts:', error);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  };

  const debugStoreContents = async () => {
    try {
      const result = await invoke('debug_store_contents') as string;
      console.log('Store debug info:', result);
    } catch (error) {
      console.error('Error debugging store:', error);
    }
  };

  return (
    <SidebarContext.Provider value={{ 
      currentMeeting, 
      setCurrentMeeting, 
      sidebarItems, 
      isCollapsed, 
      toggleCollapse, 
      meetings, 
      setMeetings, 
      isMeetingActive, 
      setIsMeetingActive,
      isRecording,
      setIsRecording,
      handleRecordingToggle,
      searchTranscripts,
      searchResults,
      isSearching,
      setServerAddress,
      serverAddress,
      transcriptServerAddress,
      setTranscriptServerAddress,
      debugStoreContents
    }}>
      {children}
    </SidebarContext.Provider>
  );
}
