'use client';
// VALUEOS: tiny inline stroke icons (currentColor) for the shell nav and controls. Inline
// SVG keeps us CSP-clean (no icon font / external asset) and dependency-free.
import React from 'react';

type P = { className?: string; size?: number };
const svg = (children: React.ReactNode, size = 18, className = 'va-ic') => (
  <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    {children}
  </svg>
);

export const IcDashboard = ({ className, size }: P) =>
  svg(
    <>
      <rect x="3" y="3" width="8" height="8" rx="1.5" />
      <rect x="13" y="3" width="8" height="5" rx="1.5" />
      <rect x="13" y="11" width="8" height="10" rx="1.5" />
      <rect x="3" y="14" width="8" height="7" rx="1.5" />
    </>,
    size,
    className,
  );

export const IcTranscripts = ({ className, size }: P) =>
  svg(
    <>
      <path d="M5 3.5h9l5 5V20a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1Z" />
      <path d="M14 3.5V8h5" />
      <path d="M8 12h8M8 16h5" />
    </>,
    size,
    className,
  );

export const IcSettings = ({ className, size }: P) =>
  svg(
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </>,
    size,
    className,
  );

export const IcMic = ({ className, size }: P) =>
  svg(
    <>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M6 11a6 6 0 0 0 12 0M12 17v4M8.5 21h7" />
    </>,
    size,
    className,
  );

export const IcFolder = ({ className, size }: P) =>
  svg(<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5A1.5 1.5 0 0 1 19.5 20h-15A1.5 1.5 0 0 1 3 18.5Z" />, size, className);

export const IcCheck = ({ className, size }: P) => svg(<path d="M4 12.5l5 5L20 6.5" />, size, className);
export const IcTrash = ({ className, size }: P) =>
  svg(<><path d="M4 7h16" /><path d="M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" /><path d="M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13" /><path d="M10 11v6M14 11v6" /></>, size, className);
export const IcClose = ({ className, size }: P) => svg(<path d="M6 6l12 12M18 6L6 18" />, size, className);
export const IcArrowRight = ({ className, size }: P) => svg(<path d="M5 12h14M13 6l6 6-6 6" />, size, className);
export const IcArrowLeft = ({ className, size }: P) => svg(<path d="M19 12H5M11 6l-6 6 6 6" />, size, className);
export const IcPlus = ({ className, size }: P) => svg(<path d="M12 5v14M5 12h14" />, size, className);
export const IcLogout = ({ className, size }: P) =>
  svg(<><path d="M15 4h3.5A1.5 1.5 0 0 1 20 5.5v13A1.5 1.5 0 0 1 18.5 20H15" /><path d="M10 12h10M17 9l3 3-3 3" /><path d="M10 4H5.5A1.5 1.5 0 0 0 4 5.5v13A1.5 1.5 0 0 0 5.5 20H10" /></>, size, className);
export const IcRefresh = ({ className, size }: P) =>
  svg(<><path d="M20 11a8 8 0 0 0-14-4.5L4 8" /><path d="M4 4v4h4" /><path d="M4 13a8 8 0 0 0 14 4.5L20 16" /><path d="M20 20v-4h-4" /></>, size, className);
