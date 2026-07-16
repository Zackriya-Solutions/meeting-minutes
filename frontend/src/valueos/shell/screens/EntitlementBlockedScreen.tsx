import React from 'react';
import * as ui from './ui';

// VALUEOS: hard entitlement block — shown when no accessible tenant has an active ValueOS
// Agent subscription. No bypass: there is no path forward to capture from here.
export const VALUEOS_PURCHASE_URL = 'https://www.value-accelerator.io';

export function EntitlementBlockedScreen({
  state,
  onContact,
  onRetry,
}: {
  state: 'expired' | 'never' | 'none';
  onContact: () => void;
  onRetry: () => void;
}) {
  const msg =
    state === 'expired'
      ? 'Your ValueOS Agent subscription has expired.'
      : "This account isn't subscribed to ValueOS Agent.";
  return (
    <div data-testid="valueos-blocked" style={ui.page}>
      <div style={ui.card}>
        <h1 style={ui.h1}>ValueOS subscription required</h1>
        <p style={ui.sub}>
          {msg} A valid ValueOS Agent subscription is required to capture and upload
          meetings. Contact Value Accelerator to get set up.
        </p>
        <button data-testid="valueos-blocked-contact" style={ui.primaryBtn} onClick={onContact}>
          Contact Value Accelerator
        </button>
        <p data-testid="valueos-blocked-url" style={{ ...ui.sub, marginTop: 16 }}>
          {VALUEOS_PURCHASE_URL}
        </p>
        <button data-testid="valueos-blocked-retry" style={ui.ghostBtn} onClick={onRetry}>
          I&apos;ve subscribed — check again
        </button>
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
