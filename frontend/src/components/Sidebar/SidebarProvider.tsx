'use client';

import React, { useState, useEffect, createContext, useContext } from 'react';
import { useRouter } from 'next/navigation';

interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

interface CurrentMeeting {
  id: string;
  title: string;
}

interface SidebarContextProps {
  isCollapsed: boolean;
  toggleCollapse: () => void;
  expandedFolders: Set<string>;
  toggleFolder: (folderId: string) => void;
  sidebarItems: SidebarItem[];
  setSidebarItems: (items: SidebarItem[]) => void;
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: (meeting: CurrentMeeting) => void;
}

const SidebarContext = createContext<SidebarContextProps | null>(null);

export const useSidebar = () => {
  const context = useContext(SidebarContext);
  if (!context) {
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return context;
};

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [expandedFolders, setExpandedFolders] = useState(new Set<string>(['meetings', 'notes']));
  const [sidebarItems, setSidebarItems] = useState<SidebarItem[]>([]);
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>(null);
  const router = useRouter();

  useEffect(() => {
    const fetchMeetings = async () => {
      try {
        const response = await fetch('http://localhost:5167/meetings'); // Add full backend URL
        if (!response.ok) throw new Error(`Failed to fetch meetings: ${response.status}`);

        const meetingsData = await response.json();
        const formattedMeetings: SidebarItem[] = meetingsData.map((meeting: any) => ({
          id: meeting.id,
          title: meeting.title,
          type: 'file',
        }));

        setSidebarItems([
          {
            id: 'meetings',
            title: 'Meetings',
            type: 'folder',
            children: formattedMeetings,
          },
        ]);
      } catch (err) {
        console.error('Error fetching meetings:', err);
      }
    };

    fetchMeetings();
  }, []);

  const toggleCollapse = () => setIsCollapsed((prev) => !prev);

  const toggleFolder = (folderId: string) => {
    setExpandedFolders((prev) => {
      const newSet = new Set(prev);
      newSet.has(folderId) ? newSet.delete(folderId) : newSet.add(folderId);
      return newSet;
    });
  };

  return (
    <SidebarContext.Provider
      value={{
        isCollapsed,
        toggleCollapse,
        expandedFolders,
        toggleFolder,
        sidebarItems,
        setSidebarItems,
        currentMeeting,
        setCurrentMeeting,
      }}
    >
      {children}
    </SidebarContext.Provider>
  );
}
