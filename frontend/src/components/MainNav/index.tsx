'use client';

import React from 'react';

interface MainNavProps {
  title: string;
}

const MainNav: React.FC<MainNavProps> = ({ title }) => {
  return (
    <div className="h-12 px-4 flex items-center border-b bg-white">
      <h1 className="text-lg font-semibold text-gray-800">
        {title}
      </h1>
    </div>
  );
};

export default MainNav;
