'use client';

import React, { useState, useEffect } from 'react';
import { 
  ChevronDown, ChevronRight, File, Settings, 
  ChevronLeftCircle, ChevronRightCircle, Calendar, StickyNote 
} from 'lucide-react';
import { useRouter } from 'next/navigation';
import { useSidebar } from './SidebarProvider';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const { sidebarItems, isCollapsed, toggleCollapse, expandedFolders, toggleFolder } = useSidebar();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchMeetings = async () => {
      try {
        setIsLoading(true);
        const response = await fetch('/meetings');
        
        if (!response.ok) {
          throw new Error(`Failed to fetch meetings: ${response.status}`);
        }
        
        setError(null);
      } catch (err) {
        console.error('Error fetching meetings:', err);
        setError('Failed to load meetings');
      } finally {
        setIsLoading(false);
      }
    };

    fetchMeetings();
  }, []);

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <button
        onClick={toggleCollapse}
        className="absolute -right-6 top-20 z-50 p-1 bg-white hover:bg-gray-100 rounded-full shadow-lg border"
        style={{ transform: 'translateX(50%)' }}
      >
        {isCollapsed ? <ChevronRightCircle className="w-6 h-6" /> : <ChevronLeftCircle className="w-6 h-6" />}
      </button>

      <div className={`h-screen bg-white border-r flex flex-col transition-all duration-300 ${isCollapsed ? 'w-16' : 'w-64'}`}>      
        <div className="h-16 flex items-center border-b">
          <div className="w-20 h-16" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
          <div className="flex-1">{!isCollapsed && <h1 className="font-semibold text-sm">Meeting Minutes</h1>}</div>
        </div>

        <div className="flex-1 overflow-y-auto">
          {isLoading && !isCollapsed && <div className="p-4 text-sm text-gray-500">Loading meetings...</div>}
          {error && !isCollapsed && <div className="p-4 text-sm text-red-500">{error}</div>}

          {sidebarItems.map(item => (
            <div key={item.id}>
              <div
                className="flex items-center px-2 py-1 hover:bg-gray-100 cursor-pointer text-sm"
                onClick={() => (item.type === 'folder' ? toggleFolder(item.id) : router.push(`/${item.id.includes('-') ? 'meetings' : 'notes'}/${item.id}`))}
              >
                {item.type === 'folder' ? (
                  <>
                    {item.id === 'meetings' ? <Calendar className="w-4 h-4 mr-2" /> : <StickyNote className="w-4 h-4 mr-2" />}
                    {expandedFolders.has(item.id) ? <ChevronDown className="w-4 h-4 mr-1" /> : <ChevronRight className="w-4 h-4 mr-1" />}
                    {item.title}
                  </>
                ) : (
                  <>
                    <File className="w-4 h-4 mr-1" />
                    {item.title}
                  </>
                )}
              </div>

              {item.type === 'folder' && expandedFolders.has(item.id) && item.children && (
                <div>{item.children.map(child => (
                  <div key={child.id} className="pl-6 flex items-center py-1 cursor-pointer text-sm hover:bg-gray-100" onClick={() => router.push(`/${child.id}`)}>
                    <File className="w-4 h-4 mr-1" />
                    {child.title}
                  </div>
                ))}</div>
              )}
            </div>
          ))}
        </div>

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
