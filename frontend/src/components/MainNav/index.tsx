'use client';

import React from 'react';
import Link from 'next/link';

interface MainNavProps {
  title: string;
}

const MainNav: React.FC<MainNavProps> = ({ title }) => {
  return (
    <div className="h-auto flex items-center border-b py-2">
      <div className="max-w-5xl mx-auto w-full px-8">
        <Link href="/">
          <h1 className="text-2xl font-semibold cursor-pointer">{title}</h1>
        </Link>
      </div>
    </div>
  );
};

export default MainNav;
