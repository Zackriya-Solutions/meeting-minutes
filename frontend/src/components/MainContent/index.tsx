'use client';

import React from 'react';

/**
 * Content column. Offsets by the live rail width via `--rail`, which AppShell
 * sets from sidebar state — so fixed-position children (the recording
 * transport, status overlays) align to the same value instead of re-deriving
 * it with inline style math.
 *
 * The collapse is instant. Animating this margin re-ran layout for every frame
 * of 260ms, and the thing being re-laid-out is a virtualized transcript that
 * can hold thousands of rows. An instrument snapping is better than an
 * instrument sliding badly. See /DESIGN.md § Motion.
 */
const MainContent: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <main className="min-w-0 flex-1" style={{ marginLeft: 'var(--rail)' }}>
    {children}
  </main>
);

export default MainContent;
