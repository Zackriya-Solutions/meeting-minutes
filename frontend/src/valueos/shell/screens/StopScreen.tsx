import React from 'react';
import { VaLogo, VA_BLUE } from '../../assets/VaLogo';

// VALUEOS: Screen C — end-of-flow stop page. The branded shell STOPS here for now.
export function StopScreen({ onContinue }: { onContinue: () => void }) {
  return (
    <div data-testid="valueos-stop" style={styles.root}>
      <div style={styles.center}>
        <VaLogo size={96} />
        <h1 style={styles.title}>Setup complete</h1>
        <p style={styles.subtitle}>
          ValueOS Agent is ready. Meeting capture is coming soon — you&apos;re all set for now.
        </p>

        {/*
          ────────────────────────────────────────────────────────────────────────
          VALUEOS HAND-OFF — NEXT FEATURE CONTINUES FROM HERE
          The next feature (meeting capture / transcription UI) will continue from this
          stop page. When built, wire it to `onContinue()` (and enable the button below).
          For now this is an intentional no-op — the flow ends here.
          ────────────────────────────────────────────────────────────────────────
        */}
        <button
          type="button"
          data-testid="valueos-handoff"
          style={styles.buttonDisabled}
          disabled
          onClick={onContinue}
        >
          Start capturing (coming soon)
        </button>
      </div>
      <footer style={styles.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  root: {
    position: 'fixed',
    inset: 0,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    background: `linear-gradient(160deg, ${VA_BLUE} 0%, #001f7a 100%)`,
    color: '#ffffff',
    fontFamily: 'system-ui, -apple-system, "Segoe UI", sans-serif',
    padding: 24,
    textAlign: 'center',
  },
  center: { maxWidth: 440, display: 'flex', flexDirection: 'column', alignItems: 'center' },
  title: { fontSize: 34, fontWeight: 800, margin: '24px 0 8px' },
  subtitle: { fontSize: 16, lineHeight: 1.5, opacity: 0.9, margin: '0 0 32px' },
  buttonDisabled: {
    background: 'rgba(255,255,255,0.25)',
    color: '#ffffff',
    border: 'none',
    borderRadius: 10,
    padding: '14px 32px',
    fontSize: 16,
    fontWeight: 700,
    cursor: 'not-allowed',
  },
  footer: { position: 'absolute', bottom: 20, fontSize: 12, opacity: 0.7, letterSpacing: 1 },
};
