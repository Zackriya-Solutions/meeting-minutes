import React from 'react';
import { VaLogo, VA_BLUE } from '../../assets/VaLogo';

// VALUEOS: Screen A — Value Accelerator branded landing / entry point.
export function LandingScreen({ onProceed }: { onProceed: () => void }) {
  return (
    <div data-testid="valueos-landing" style={styles.root}>
      <div style={styles.center}>
        <VaLogo size={112} />
        <h1 style={styles.title}>ValueOS Agent</h1>
        <p style={styles.subtitle}>
          Your private, on-device meeting agent. Captures and transcribes meetings
          locally, then feeds them into your ValueOS workflows.
        </p>

        {/*
          ────────────────────────────────────────────────────────────────────────
          VALUEOS SEAM — FUTURE LOGIN / SUBSCRIPTION GATE
          A login/subscription step will be inserted HERE, between this landing page
          (Screen A) and model download (Screen B). When implemented, it should run on
          "Get started" and only call `onProceed()` after successful auth. For now this
          is a no-op passthrough — do NOT implement login in this feature.
          ────────────────────────────────────────────────────────────────────────
        */}

        <button
          type="button"
          data-testid="valueos-proceed"
          style={styles.button}
          onClick={onProceed}
        >
          Get started
        </button>
      </div>
      <footer style={styles.footer}>Value Accelerator</footer>
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
  center: { maxWidth: 460, display: 'flex', flexDirection: 'column', alignItems: 'center' },
  title: { fontSize: 40, fontWeight: 800, margin: '24px 0 8px' },
  subtitle: { fontSize: 16, lineHeight: 1.5, opacity: 0.9, margin: '0 0 32px' },
  button: {
    background: '#ffffff',
    color: VA_BLUE,
    border: 'none',
    borderRadius: 10,
    padding: '14px 32px',
    fontSize: 16,
    fontWeight: 700,
    cursor: 'pointer',
  },
  footer: { position: 'absolute', bottom: 20, fontSize: 12, opacity: 0.7, letterSpacing: 1 },
};
