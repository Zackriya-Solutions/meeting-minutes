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
      className={`min-h-screen min-w-0 flex-1 overflow-hidden bg-background text-foreground transition-all duration-300 ${
        isCollapsed ? 'pl-16' : 'pl-64'
      }`}
    >
      <div className="min-w-0 pl-4 sm:pl-8">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
