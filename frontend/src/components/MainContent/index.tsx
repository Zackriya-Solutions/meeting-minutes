import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <div className="w-full h-full flex-1 overflow-auto">
      {children}
    </div>
  );
};

export default MainContent;