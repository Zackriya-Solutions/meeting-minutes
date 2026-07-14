'use client';

import React from 'react';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  const { isCollapsed, sidebarWidth, isSidebarResizing } = useSidebar();

  return (
    <main
      className={`flex-1 ${isSidebarResizing ? '' : 'transition-all duration-300'}`}
      style={{ marginLeft: isCollapsed ? '4rem' : sidebarWidth }}
    >
      <div>
        {children}
      </div>
    </main>
  );
};

export default MainContent;
