'use client';

import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <main 
      className="min-h-screen min-w-0 flex-1 overflow-hidden bg-background text-foreground transition-all duration-300"
    >
      <div className="min-w-0 pl-4 sm:pl-8">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
