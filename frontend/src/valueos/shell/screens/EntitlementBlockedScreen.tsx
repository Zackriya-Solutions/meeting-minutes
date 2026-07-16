import React from 'react';
import * as ui from './ui';

// VALUEOS: hard entitlement block — shown when GET /me/agent-tenants returns no workspace
// (contract §2). No bypass: there is no path forward to capture from here. The wording is
// driven by whether the user belongs to no workspace at all vs. workspaces without the add-on.
export const VALUEOS_PURCHASE_URL = 'https://www.value-accelerator.io';

export function EntitlementBlockedScreen({
  reason,
  onContact,
  onRetry,
}: {
  reason: 'no-membership' | 'no-addon';
  onContact: () => void;
  onRetry: () => void;
}) {
  const msg =
    reason === 'no-membership'
      ? "You don't belong to any ValueOS workspace yet."
      : 'None of your workspaces have the ValueOS Agent add-on. Ask an admin to enable it.';
  return (
    <div data-testid="valueos-blocked" style={ui.page}>
      <div style={ui.card}>
        <h1 style={ui.h1}>ValueOS Agent access required</h1>
        <p style={ui.sub}>
          {msg} A workspace with an active ValueOS Agent add-on is required to capture and
          upload meetings. Contact Value Accelerator to get set up.
        </p>
        <button data-testid="valueos-blocked-contact" style={ui.primaryBtn} onClick={onContact}>
          Contact Value Accelerator
        </button>
        <p data-testid="valueos-blocked-url" style={{ ...ui.sub, marginTop: 16 }}>
          {VALUEOS_PURCHASE_URL}
        </p>
        <button data-testid="valueos-blocked-retry" style={ui.ghostBtn} onClick={onRetry}>
          Check access again
        </button>
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
