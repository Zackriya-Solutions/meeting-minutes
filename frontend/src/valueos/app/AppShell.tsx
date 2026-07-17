'use client';
// VALUEOS: the persistent app frame for all main screens — dark left sidebar (brand + nav +
// Settings pinned) and a light content area. Onboarding screens render OUTSIDE this frame
// (full-bleed blue). Nav is Dashboard / Transcripts only; there is deliberately no "Live
// Call" nav item (UI_GUIDE §2) — a live call is reached from the on-air banner/row.
import React from 'react';
import { VaMark } from '../ds/ds';
import { IcDashboard, IcTranscripts, IcSettings } from './icons';

export type MainRoute = 'dashboard' | 'transcripts' | 'settings' | 'recording';

export function AppShell({
  route,
  onNavigate,
  children,
}: {
  route: MainRoute;
  onNavigate: (r: MainRoute) => void;
  children: React.ReactNode;
}) {
  const navActive = (r: MainRoute) => (route === r ? 'va-navitem on' : 'va-navitem');
  return (
    <div className="va-shell va-root" data-testid="valueos-shell-main">
      <aside className="va-sidebar">
        <div className="va-brand">
          <VaMark height={22} tone="white" />
          <div>
            <div className="bn">ValueOS</div>
            <div className="bs">by Value Accelerator</div>
          </div>
        </div>

        <nav className="va-navlist">
          <button className={navActive('dashboard')} data-testid="valueos-nav-dashboard" onClick={() => onNavigate('dashboard')}>
            <IcDashboard /> Dashboard
          </button>
          <button className={navActive('transcripts')} data-testid="valueos-nav-transcripts" onClick={() => onNavigate('transcripts')}>
            <IcTranscripts /> Transcripts
          </button>
        </nav>

        <div className="va-navspacer" />

        <nav className="va-navlist">
          <button className={navActive('settings')} data-testid="valueos-nav-settings" onClick={() => onNavigate('settings')}>
            <IcSettings /> Settings
          </button>
        </nav>
      </aside>

      <main className="va-content va-scroll">{children}</main>
    </div>
  );
}
