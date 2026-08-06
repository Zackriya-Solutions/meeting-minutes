'use client';

import React from 'react';

/**
 * Content column. Offsets by the live rail width via `--rail`, which AppShell
 * sets from sidebar state — so fixed-position children (the recording
 * transport, status overlays) align to the same value instead of re-deriving
 * it with inline style math.
 */
const MainContent: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <main
    className="min-w-0 flex-1 transition-[margin] duration-slow"
    style={{ marginLeft: 'var(--rail)' }}
  >
    {children}
  </main>
);

export default MainContent;
