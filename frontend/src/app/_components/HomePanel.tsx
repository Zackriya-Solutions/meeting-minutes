import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { GlobeIcon } from '@/components/deslop-icons';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { useT } from '@/lib/i18n';
import { HomeMeetingList } from './HomeMeetingList';

/**
 * HomePanel Component
 *
 * Scrollable body of the home route: the meeting archive plus the pre-recording
 * permission notice.
 *
 * A live transcript is deliberately NOT rendered here. It belongs to the
 * `/recording` route drawer, which floats as a right-hand panel over this list.
 * Rendering it on home as well meant that dismissing the drawer mid-recording
 * handed the whole window to the transcript and hid the archive behind it,
 * while returning through the recording affordance showed the same transcript as a
 * panel. Keeping this view list-only also makes it identical to the background
 * the recording/meeting drawers render, so the hand-off has nothing to redraw.
 */

interface HomePanelProps {
  showModal: (name: ModalType, message?: string) => void;
}

export function HomePanel({ showModal }: HomePanelProps) {
  const t = useT();
  const { transcriptModelConfig } = useConfig();
  const { isRecording } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone } = usePermissionCheck();
  const isLinux = useIsLinux();

  return (
    <div
      data-home-scroll-container
      className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto bg-[var(--elevation-1)]"
    >
      {/* Whisper's transcription language has no other entry point in the app,
          so the picker stays reachable from home for that provider only. */}
      {transcriptModelConfig.provider === 'localWhisper' && (
        <div className="sticky top-0 z-10 flex justify-center bg-[var(--elevation-1)] p-4">
          <Button
            variant="outline"
            size="sm"
            onClick={() => showModal('languageSettings')}
            title={t('Language')}
          >
            <GlobeIcon />
            <span className="hidden md:inline">{t('Language')}</span>
          </Button>
        </div>
      )}

      {/* Permission Warning - Not needed on Linux */}
      {!isRecording &&
        !isChecking &&
        !isLinux &&
        (!hasMicrophone || !hasSystemAudio) && (
        <div className="flex justify-center px-4 pt-4">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      <HomeMeetingList animateOnMount={false} />
    </div>
  );
}
