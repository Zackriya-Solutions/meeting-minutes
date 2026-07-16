import React from 'react';
import { BUILD_INFO, type ValueOsBuildInfo } from '../buildInfo';

// VALUEOS: a small, unobtrusive build stamp for the corner of a screen. It renders in ALL
// builds (including the packaged/release app) on purpose — the whole point is to tell at a
// glance which commit an *installed* build came from, so a stale build is instantly obvious.
// It's low-contrast, tiny, and click-through (pointerEvents: none) so it never gets in the
// way of the UI beneath it. `info` is injectable for testing; it defaults to the ambient
// build identity baked in at build time.
export function BuildStamp({
  style,
  info = BUILD_INFO,
}: {
  style?: React.CSSProperties;
  info?: ValueOsBuildInfo;
}) {
  return (
    <span data-testid="valueos-build-stamp" style={{ ...base, ...style }}>
      {info.label}
    </span>
  );
}

const base: React.CSSProperties = {
  position: 'fixed',
  right: 10,
  bottom: 8,
  zIndex: 9999,
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  fontSize: 11,
  lineHeight: 1,
  letterSpacing: 0.2,
  whiteSpace: 'nowrap',
  color: 'rgba(255, 255, 255, 0.7)',
  pointerEvents: 'none',
  userSelect: 'none',
};
