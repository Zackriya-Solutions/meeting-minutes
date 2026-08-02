'use client';

import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <main className="h-screen min-w-0 flex-1 overflow-hidden">
      <div className="h-full min-w-0">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
