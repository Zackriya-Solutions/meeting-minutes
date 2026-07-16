'use client';

import React from 'react';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  const { isCollapsed, sidebarWidth, isSidebarResizing } = useSidebar();
  const sidebarOffset = isCollapsed ? 64 : sidebarWidth;

  return (
    <main
      className={`h-screen min-w-0 overflow-hidden ${
        isSidebarResizing ? '' : 'transition-[margin-left,width] duration-300'
      }`}
      style={{
        marginLeft: sidebarOffset,
        width: `calc(100vw - ${sidebarOffset}px)`,
      }}
    >
      <div className="h-full min-w-0">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
