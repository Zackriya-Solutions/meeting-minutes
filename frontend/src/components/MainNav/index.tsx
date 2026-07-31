'use client';

import React from 'react';

interface MainNavProps {
  title: string;
}

const MainNav: React.FC<MainNavProps> = ({ title }) => {
  return (
    <div className="h-0 flex items-center border-b">
      <div className="max-w-5xl mx-auto w-full px-8">
        <h1 className="memento-screen-title">{title}</h1>
      </div>
    </div>
  );
};

export default MainNav;
