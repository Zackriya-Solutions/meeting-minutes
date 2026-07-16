import React, { useEffect, useRef } from 'react';
import { VA_BLUE } from '../../assets/VaLogo';
// VALUEOS: REUSE upstream's model-download capability as-is (no copy). `useOnboarding`
// is an exported hook whose provider wraps every route, so we call it directly. It runs
// the real Tauri downloads; we only show a branded, detail-free loading screen over it.
import { useOnboarding } from '@/contexts/OnboardingContext';

// VALUEOS: Screen B — branded setup/loading. Blue full screen, a spinner, and generic
// copy only. We deliberately do NOT surface per-model names, sources, or progress
// breakdowns (nothing that reveals the underlying engine).
export function ModelDownloadScreen({ onComplete }: { onComplete: () => void }) {
  const {
    startBackgroundDownloads,
    recommendedSummaryModel,
    parakeetDownloaded,
    summaryModelDownloaded,
  } = useOnboarding();

  const complete = parakeetDownloaded && summaryModelDownloaded;
  const startedRef = useRef(false);

  // Auto-start the reused download once (as soon as the recommended model is known).
  useEffect(() => {
    if (complete || startedRef.current || !recommendedSummaryModel) return;
    startedRef.current = true;
    void startBackgroundDownloads({
      includeParakeet: true,
      includeSummary: true,
      summaryModel: recommendedSummaryModel,
    });
  }, [complete, recommendedSummaryModel, startBackgroundDownloads]);

  // Advance to the stop page once the reused download reports everything ready.
  useEffect(() => {
    if (complete) onComplete();
  }, [complete, onComplete]);

  return (
    <div data-testid="valueos-download" style={styles.root}>
      <style>{`@keyframes valueos-spin { to { transform: rotate(360deg); } }`}</style>
      <div style={styles.center}>
        <div data-testid="valueos-spinner" style={styles.spinner} aria-hidden="true" />
        <h1 style={styles.title}>Setting up ValueOS Agent</h1>
        <p data-testid="valueos-download-status" style={styles.status}>
          Downloading models…
        </p>
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
  center: { display: 'flex', flexDirection: 'column', alignItems: 'center' },
  spinner: {
    width: 56,
    height: 56,
    borderRadius: '50%',
    border: '4px solid rgba(255,255,255,0.25)',
    borderTopColor: '#ffffff',
    animation: 'valueos-spin 0.9s linear infinite',
  },
  title: { fontSize: 28, fontWeight: 800, margin: '28px 0 8px' },
  status: { fontSize: 16, opacity: 0.85, margin: 0 },
  footer: { position: 'absolute', bottom: 20, fontSize: 12, opacity: 0.7, letterSpacing: 1 },
};
