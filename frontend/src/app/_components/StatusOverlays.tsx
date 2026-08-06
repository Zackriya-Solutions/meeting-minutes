import { Loader2 } from 'lucide-react';

interface StatusOverlaysProps {
  isProcessing: boolean; // Finalizing transcription after recording stops
  isSaving: boolean;     // Writing transcript to the local database
  /** Retained for call-site compatibility; offset now comes from --rail. */
  sidebarCollapsed?: boolean;
}

function StatusOverlay({ show, message }: { show: boolean; message: string }) {
  if (!show) return null;

  return (
    <div
      role="status"
      aria-live="polite"
      className="fixed bottom-8 left-0 right-0 z-sticky flex justify-center px-6"
      style={{ paddingLeft: 'calc(var(--rail) + 1.5rem)' }}
    >
      <div className="flex items-center gap-2.5 rounded-lg border border-line bg-elevated px-3.5 py-2 shadow-float">
        <Loader2 className="h-4 w-4 shrink-0 animate-spin text-ink-muted" aria-hidden />
        <span className="text-base text-ink">{message}</span>
      </div>
    </div>
  );
}

export function StatusOverlays({ isProcessing, isSaving }: StatusOverlaysProps) {
  return (
    <>
      <StatusOverlay show={isProcessing} message="Finalizing transcription…" />
      <StatusOverlay show={isSaving} message="Saving to this machine…" />
    </>
  );
}
