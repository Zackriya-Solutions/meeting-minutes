import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ValueOsShell } from '@/valueos/shell/ValueOsShell';
import { valueOsShellEnabled } from '@/valueos/shell/flag';

// Mock the UPSTREAM model-download capability our shell reuses. We assert our shell
// drives it correctly; we do not test the real download (that's upstream's code).
const h = vi.hoisted(() => ({
  state: {
    currentStep: 1,
    parakeetDownloaded: false,
    parakeetProgress: 0,
    parakeetProgressInfo: { percent: 0, downloadedMb: 0, totalMb: 0, speedMbps: 0 },
    summaryModelDownloaded: false,
    summaryModelProgress: 0,
    summaryModelProgressInfo: { percent: 0, downloadedMb: 0, totalMb: 0, speedMbps: 0 },
    selectedSummaryModel: 'rec-model',
    recommendedSummaryModel: 'rec-model',
    databaseExists: true,
    isBackgroundDownloading: false,
  },
  startBackgroundDownloads: vi.fn(() => Promise.resolve()),
  retryParakeetDownload: vi.fn(() => Promise.resolve()),
}));

vi.mock('@/contexts/OnboardingContext', () => ({
  useOnboarding: () => ({
    ...h.state,
    goToStep: vi.fn(),
    goNext: vi.fn(),
    goPrevious: vi.fn(),
    setParakeetDownloaded: vi.fn(),
    setSummaryModelDownloaded: vi.fn(),
    setSelectedSummaryModel: vi.fn(),
    setDatabaseExists: vi.fn(),
    setPermissionStatus: vi.fn(),
    setPermissionsSkipped: vi.fn(),
    completeOnboarding: vi.fn(() => Promise.resolve()),
    startBackgroundDownloads: h.startBackgroundDownloads,
    retryParakeetDownload: h.retryParakeetDownload,
  }),
}));

beforeEach(() => {
  h.state.parakeetDownloaded = false;
  h.state.summaryModelDownloaded = false;
  h.state.parakeetProgress = 0;
  h.state.summaryModelProgress = 0;
  h.startBackgroundDownloads.mockClear();
  h.retryParakeetDownload.mockClear();
});

describe('ValueOS branded shell', () => {
  it('is enabled as the entry point by default', () => {
    expect(valueOsShellEnabled).toBe(true);
  });

  it('A: renders Value Accelerator branding on the landing screen', () => {
    render(<ValueOsShell />);
    expect(screen.getByTestId('valueos-landing')).toBeInTheDocument();
    expect(screen.getByText('ValueOS Agent')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /value accelerator/i })).toBeInTheDocument();
    expect(screen.getByText('Value Accelerator GmbH')).toBeInTheDocument();
    expect(screen.getByTestId('valueos-proceed')).toBeInTheDocument();
  });

  it('A→B: the proceed control advances to the model-download screen', () => {
    render(<ValueOsShell />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    expect(screen.getByTestId('valueos-download')).toBeInTheDocument();
    expect(screen.queryByTestId('valueos-landing')).not.toBeInTheDocument();
  });

  it('B: entering the download screen auto-triggers the reused capability with the right args', async () => {
    render(<ValueOsShell />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    await waitFor(() => expect(h.startBackgroundDownloads).toHaveBeenCalledTimes(1));
    expect(h.startBackgroundDownloads).toHaveBeenCalledWith({
      includeParakeet: true,
      includeSummary: true,
      summaryModel: 'rec-model',
    });
  });

  it('B: shows a generic loading state and never leaks engine/model details', () => {
    render(<ValueOsShell />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    expect(screen.getByTestId('valueos-spinner')).toBeInTheDocument();
    expect(screen.getByTestId('valueos-download-status')).toHaveTextContent(/downloading models/i);
    // No details that would reveal the underlying engine.
    expect(screen.queryByText(/parakeet|meetily|whisper|transcription model|summary model/i)).toBeNull();
  });

  it('B→C: completing the download advances to the stop page and ends the flow', async () => {
    const { rerender } = render(<ValueOsShell />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    await waitFor(() => expect(h.startBackgroundDownloads).toHaveBeenCalled());

    // Simulate the reused download reporting both models ready.
    h.state.parakeetDownloaded = true;
    h.state.summaryModelDownloaded = true;
    rerender(<ValueOsShell />);

    await waitFor(() => expect(screen.getByTestId('valueos-stop')).toBeInTheDocument());
    expect(screen.getByText('Setup complete')).toBeInTheDocument();
    // The flow ends here: the hand-off control is a disabled no-op.
    expect(screen.getByTestId('valueos-handoff')).toBeDisabled();
    expect(screen.queryByTestId('valueos-download')).not.toBeInTheDocument();
  });
});
