'use client';

import { usePathname, useRouter } from 'next/navigation';
import { AnimatePresence, motion } from 'framer-motion';
import { Button } from '@/components/ui/button';
import { RecordingStatus, useRecordingState } from '@/contexts/RecordingStateContext';
import { useRecordingSessionStop } from '@/contexts/RecordingPostProcessingProvider';
import { useT } from '@/lib/i18n';
import { RECORDING_ROUTE, shouldShowRecordingPill } from '@/lib/recordingNavigation';

function formatDuration(totalSeconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = safeSeconds % 60;
  const pad = (value: number) => value.toString().padStart(2, '0');

  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}

/**
 * Floating "recording in progress" pill shown on every route except the recording
 * screen itself.
 *
 * Recording used to pin the user to `/recording` — a route guard bounced every other
 * page back. Navigation is free now, which makes this the only way back to a live
 * session and the only Finish button off that route, so it renders through teardown
 * as well as capture.
 */
export function GlobalRecordingPill() {
  const pathname = usePathname();
  const router = useRouter();
  const t = useT();
  const { isRecording, isPaused, status, activeDuration } = useRecordingState();
  const { stopRecording } = useRecordingSessionStop();

  const visible = shouldShowRecordingPill(pathname, isRecording, status);
  const isFinalizing =
    status === RecordingStatus.STOPPING ||
    status === RecordingStatus.PROCESSING_TRANSCRIPTS ||
    status === RecordingStatus.SAVING;
  const isStarting = status === RecordingStatus.STARTING;

  const label = isFinalizing
    ? t('Saving meeting…')
    : isStarting
      ? t('Starting recording…')
      : isPaused
        ? t('Paused')
        : t('Recording');

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          transition={{ duration: 0.18 }}
          className="fixed bottom-8 right-8 z-50"
        >
          <div className="flex items-center gap-1 rounded-full border border-[var(--primary-10)] bg-[var(--elevation-2)] py-1.5 pl-3 pr-1.5 shadow-lg">
            <button
              type="button"
              onClick={() => router.push(RECORDING_ROUTE)}
              aria-label={t('Return to recording')}
              title={t('Return to recording')}
              className="mm-hover flex cursor-pointer items-center gap-2 rounded-full bg-transparent px-2 py-1 text-sm font-medium text-foreground"
            >
              <span
                aria-hidden="true"
                className={`size-2 shrink-0 rounded-full ${
                  isFinalizing
                    ? 'bg-muted-foreground'
                    : isPaused
                      ? 'bg-primary'
                      : 'bg-destructive animate-pulse'
                }`}
              />
              <span>{label}</span>
              {!isFinalizing && !isStarting && (
                <span className="tabular-nums text-muted-foreground">
                  {formatDuration(activeDuration ?? 0)}
                </span>
              )}
            </button>

            <Button
              type="button"
              size="sm"
              onClick={stopRecording}
              disabled={isFinalizing}
              aria-busy={isFinalizing}
              aria-label={isFinalizing ? t('Saving meeting…') : t('Finish')}
              className="h-8 rounded-full px-3 text-sm disabled:opacity-100"
            >
              {isFinalizing ? (
                <>
                  <span
                    aria-hidden="true"
                    className="block size-4 origin-center animate-spin rounded-full border-2 border-current border-r-transparent"
                  />
                  <span className="sr-only">{t('Saving meeting…')}</span>
                </>
              ) : t('Finish')}
            </Button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
