import { useT } from '@/lib/i18n';
import { LoaderCircle } from '@/components/deslop-icons';
import { Card } from '@/components/ui/card';

interface StatusOverlaysProps {
  // Status flags
  isProcessing: boolean;      // Processing transcription after recording stops
  isSaving: boolean;          // Saving transcript to database
}

// Internal reusable component for individual status overlays
interface StatusOverlayProps {
  show: boolean;
  message: string;
}

function StatusOverlay({ show, message }: StatusOverlayProps) {
  if (!show) return null;

  return (
    <div className="fixed bottom-4 left-0 right-0 z-10">
      <div className="flex justify-center">
        <div className="w-2/3 max-w-[750px] flex justify-center">
          <Card className="flex items-center gap-2 px-4 py-2">
            <LoaderCircle className="size-4 animate-spin" />
            <span className="text-sm text-muted-foreground">{message}</span>
          </Card>
        </div>
      </div>
    </div>
  );
}

// Main exported component - renders multiple status overlays
export function StatusOverlays({
  isProcessing,
  isSaving
}: StatusOverlaysProps) {
  const t = useT();
  return (
    <>
      {/* Processing status overlay - shown after recording stops while finalizing transcription */}
      <StatusOverlay
        show={isProcessing}
        message={t('Finalizing transcription...')}
      />

      {/* Saving status overlay - shown while saving transcript to database */}
      <StatusOverlay
        show={isSaving}
        message={t('Saving transcript...')}
      />
    </>
  );
}
