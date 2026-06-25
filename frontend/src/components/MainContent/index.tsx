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
      className={`min-h-screen flex-1 bg-background text-foreground transition-all duration-300 ${
        isCollapsed ? 'ml-16' : 'ml-64'
      }`}
    >
      <div className="pl-8">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
