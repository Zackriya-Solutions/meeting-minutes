'use client';

import React from 'react';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  const { isCollapsed } = useSidebar();

  return (
    <main 
      className={`flex-1 min-w-0 h-screen overflow-hidden bg-[var(--pt-bg)] transition-all duration-300 ${
        isCollapsed ? 'ml-16' : 'ml-64'
      }`}
    >
      <div className="h-full">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
