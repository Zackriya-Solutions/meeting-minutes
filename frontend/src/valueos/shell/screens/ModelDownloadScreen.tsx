import React, { useEffect, useState } from 'react';
import { VA_BLUE } from '../../assets/VaLogo';
// VALUEOS: REUSE upstream's model-download capability as-is (no copy). `useOnboarding`
// is an exported hook whose provider (OnboardingProvider) wraps every route, so we call
// it directly. It performs the real Tauri downloads (parakeet + summary model); we only
// wrap it in our branded UI. See valueos/FEATURE-branded-shell.md.
import { useOnboarding } from '@/contexts/OnboardingContext';

// VALUEOS: Screen B — ValueOS-branded model download (reuses upstream download).
export function ModelDownloadScreen({ onComplete }: { onComplete: () => void }) {
  const {
    startBackgroundDownloads,
    retryParakeetDownload,
    recommendedSummaryModel,
    parakeetProgress,
    parakeetDownloaded,
    summaryModelProgress,
    summaryModelDownloaded,
  } = useOnboarding();

  const [started, setStarted] = useState(false);
  const complete = parakeetDownloaded && summaryModelDownloaded;

  // Advance to the stop page once the reused download reports both models ready.
  useEffect(() => {
    if (complete) onComplete();
  }, [complete, onComplete]);

  const handleStart = async () => {
    setStarted(true);
    await startBackgroundDownloads({
      includeParakeet: true,
      includeSummary: true,
      summaryModel: recommendedSummaryModel,
    });
  };

  return (
    <div data-testid="valueos-download" style={styles.root}>
      <div style={styles.card}>
        <h1 style={styles.title}>Setting up ValueOS Agent</h1>
        <p style={styles.subtitle}>
          Download the on-device models that power local transcription and summaries.
          Everything runs on your machine.
        </p>

        {!started && !complete && (
          <button
            type="button"
            data-testid="valueos-download-start"
            style={styles.button}
            onClick={handleStart}
          >
            Download models
          </button>
        )}

        {started && !complete && (
          <div data-testid="valueos-download-progress" style={{ width: '100%' }}>
            <ProgressRow label="Transcription model" percent={parakeetProgress} />
            <ProgressRow label="Summary model" percent={summaryModelProgress} />
            <button
              type="button"
              data-testid="valueos-download-retry"
              style={styles.retry}
              onClick={() => retryParakeetDownload()}
            >
              Retry
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function ProgressRow({ label, percent }: { label: string; percent: number }) {
  const pct = Math.max(0, Math.min(100, Math.round(percent)));
  return (
    <div style={{ margin: '16px 0', textAlign: 'left' }}>
      <div style={styles.progressLabel}>
        <span>{label}</span>
        <span>{pct}%</span>
      </div>
      <div style={styles.track}>
        <div style={{ ...styles.fill, width: `${pct}%` }} />
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  root: {
    position: 'fixed',
    inset: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    background: '#f4f6fc',
    color: '#0b1533',
    fontFamily: 'system-ui, -apple-system, "Segoe UI", sans-serif',
    padding: 24,
  },
  card: {
    maxWidth: 480,
    width: '100%',
    background: '#ffffff',
    borderRadius: 16,
    padding: 40,
    boxShadow: '0 12px 40px rgba(0,32,122,0.12)',
    textAlign: 'center',
  },
  title: { fontSize: 28, fontWeight: 800, margin: '0 0 8px' },
  subtitle: { fontSize: 15, lineHeight: 1.5, opacity: 0.8, margin: '0 0 28px' },
  button: {
    background: VA_BLUE,
    color: '#ffffff',
    border: 'none',
    borderRadius: 10,
    padding: '14px 32px',
    fontSize: 16,
    fontWeight: 700,
    cursor: 'pointer',
  },
  retry: {
    background: 'transparent',
    color: VA_BLUE,
    border: 'none',
    fontSize: 14,
    fontWeight: 600,
    cursor: 'pointer',
    marginTop: 8,
  },
  progressLabel: { display: 'flex', justifyContent: 'space-between', fontSize: 14, marginBottom: 6 },
  track: { height: 8, borderRadius: 4, background: '#e2e8f5', overflow: 'hidden' },
  fill: { height: '100%', background: VA_BLUE, transition: 'width 0.3s ease' },
};
