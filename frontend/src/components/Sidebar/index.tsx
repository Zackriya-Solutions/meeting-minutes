'use client';

import React, { useState, useEffect } from 'react';
import { ChevronDown, ChevronRight, File, Settings, ChevronLeftCircle, ChevronRightCircle, Calendar, StickyNote } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { useSidebar } from './SidebarProvider';

interface Meeting {
  id: string;
  title: string;
  date: string;
  time?: string;
  attendees: string[];
  tags: string[];
  content: string;
  created_at: string;
  updated_at: string;
}

interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

const Sidebar: React.FC = () => {
  const router = useRouter();
  const { sidebarItems, isCollapsed, toggleCollapse } = useSidebar();
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['meetings', 'notes']));
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [dynamicSidebarItems, setDynamicSidebarItems] = useState<SidebarItem[]>([]);

  useEffect(() => {
    const fetchMeetings = async () => {
      try {
        setIsLoading(true);
        const response = await fetch('http://localhost:5167/meetings');
        
        if (!response.ok) {
          throw new Error(`Failed to fetch meetings: ${response.status}`);
        }
        
        const data = await response.json();
        setMeetings(data);
        
        // Transform meetings into sidebar items format
        const meetingItems: SidebarItem[] = data.map((meeting: Meeting) => ({
          id: meeting.id,
          title: meeting.title,
          type: 'file'
        }));
        
        // Create the meetings folder with children
        const meetingsFolder: SidebarItem = {
          id: 'meetings',
          title: 'Meetings',
          type: 'folder',
          children: meetingItems
        };
        
        // Keep the notes folder from original sidebarItems
        const notesFolder = sidebarItems.find(item => item.id === 'notes');
        
        // Set the dynamic sidebar items
        setDynamicSidebarItems([meetingsFolder, ...(notesFolder ? [notesFolder] : [])]);
        
        setError(null);
      } catch (err) {
        console.error('Error fetching meetings:', err);
        setError('Failed to load meetings');
        // Fallback to original sidebar items
        setDynamicSidebarItems(sidebarItems);
      } finally {
        setIsLoading(false);
      }
    };

    fetchMeetings();
  }, [sidebarItems]);

  const toggleFolder = (folderId: string) => {
    const newExpanded = new Set(expandedFolders);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    setExpandedFolders(newExpanded);
  };

  const renderCollapsedIcons = () => {
    if (!isCollapsed) return null;

    return (
      <div className="flex flex-col items-center space-y-4 mt-4">
        <button
          onClick={() => {
            if (isCollapsed) toggleCollapse();
            toggleFolder('meetings');
          }}
          className="p-2 hover:bg-gray-100 rounded-md transition-colors"
          title="Meetings"
        >
          <Calendar className="w-5 h-5 text-gray-600" />
        </button>
        <button
          onClick={() => {
            if (isCollapsed) toggleCollapse();
            toggleFolder('notes');
          }}
          className="p-2 hover:bg-gray-100 rounded-md transition-colors"
          title="Notes"
        >
          <StickyNote className="w-5 h-5 text-gray-600" />
        </button>
      </div>
    );
  };

  const renderItem = (item: SidebarItem, depth = 0) => {
    const isExpanded = expandedFolders.has(item.id);
    const paddingLeft = `${depth * 12 + 12}px`;

    if (isCollapsed) return null;

    return (
      <div key={item.id}>
        <div
          className="flex items-center px-2 py-1 hover:bg-gray-100 cursor-pointer text-sm"
          style={{ paddingLeft }}
          onClick={() => {
            if (item.type === 'folder') {
              toggleFolder(item.id);
            } else {
              const basePath = item.id.startsWith('intro-call') ? '/' : `/${item.id.includes('-') ? 'meetings' : 'notes'}/${item.id}`;
              router.push(basePath);
            }
          }}
        >
          {item.type === 'folder' ? (
            <>
              {item.id === 'meetings' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : item.id === 'notes' ? (
                <StickyNote className="w-4 h-4 mr-2" />
              ) : null}
              {isExpanded ? (
                <ChevronDown className="w-4 h-4 mr-1" />
              ) : (
                <ChevronRight className="w-4 h-4 mr-1" />
              )}
              {item.title}
            </>
          ) : (
            <>
              <File className="w-4 h-4 mr-1" />
              {item.title}
            </>
          )}
        </div>
        {item.type === 'folder' && isExpanded && item.children && (
          <div>
            {item.children.map(child => renderItem(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      {/* Floating collapse button */}
      <button
        onClick={toggleCollapse}
        className="absolute -right-6 top-20 z-50 p-1 bg-white hover:bg-gray-100 rounded-full shadow-lg border"
        style={{ transform: 'translateX(50%)' }}
      >
        {isCollapsed ? (
          <ChevronRightCircle className="w-6 h-6" />
        ) : (
          <ChevronLeftCircle className="w-6 h-6" />
        )}
      </button>

      <div 
        className={`h-screen bg-white border-r flex flex-col transition-all duration-300 ${
          isCollapsed ? 'w-16' : 'w-64'
        }`}
      >
        {/* Header with traffic light spacing */}
        <div className="h-16 flex items-center border-b">
          {/* Traffic light spacing */}
          <div className="w-20 h-16" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
          
          {/* Title container */}
          <div className="flex-1">
            {!isCollapsed && (
              <h1 className="font-semibold text-sm">Meeting Minutes</h1>
            )}
          </div>
        </div>

        {/* Main content */}
        <div className="flex-1 overflow-y-auto">
          {isLoading && !isCollapsed && (
            <div className="p-4 text-sm text-gray-500">Loading meetings...</div>
          )}
          {error && !isCollapsed && (
            <div className="p-4 text-sm text-red-500">{error}</div>
          )}
          {renderCollapsedIcons()}
          {dynamicSidebarItems.map(item => renderItem(item))}
        </div>

        {/* Footer */}
        {!isCollapsed && (
          <div className="p-4 border-t">
            <button 
              onClick={() => router.push('/settings')}
              className="w-full flex items-center px-3 py-2 text-sm text-gray-600 hover:bg-gray-100 rounded-md transition-colors"
            >
              <Settings className="w-4 h-4 mr-3" />
              <span>Settings</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export default Sidebar;
