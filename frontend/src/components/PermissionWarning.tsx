import React from 'react';
import { AlertTriangle, Mic, Speaker, RefreshCw } from '@/components/memento/LucideCompat';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { invoke } from '@tauri-apps/api/core';
import { useIsLinux } from '@/hooks/usePlatform';
import { useT } from '@/lib/i18n';

interface PermissionWarningProps {
  hasMicrophone: boolean;
  hasSystemAudio: boolean;
  onRecheck: () => void;
  isRechecking?: boolean;
}

export function PermissionWarning({
  hasMicrophone,
  hasSystemAudio,
  onRecheck,
  isRechecking = false
}: PermissionWarningProps) {
  const t = useT();
  const isLinux = useIsLinux();

  // Don't show on Linux - permission handling is not needed
  if (isLinux) {
    return null;
  }

  // Don't show if both permissions are granted
  if (hasMicrophone && hasSystemAudio) {
    return null;
  }

  const isMacOS = navigator.userAgent.includes('Mac');

  const openMicrophoneSettings = async () => {
    if (isMacOS) {
      try {
        await invoke('open_system_settings', { preferencePane: 'Privacy_Microphone' });
      } catch (error) {
        console.error('Failed to open microphone settings:', error);
      }
    }
  };

  const openScreenRecordingSettings = async () => {
    if (isMacOS) {
      try {
        await invoke('open_system_settings', { preferencePane: 'Privacy_ScreenCapture' });
      } catch (error) {
        console.error('Failed to open screen recording settings:', error);
      }
    }
  };

  return (
    <div className="max-w-md mb-4 space-y-3">
      {/* Combined Permission Warning - Show when either permission is missing */}
      {(!hasMicrophone || !hasSystemAudio) && (
        <Alert variant="destructive" className="border-[var(--gold-border)] bg-[var(--gold-soft)]">
          <AlertTriangle className="h-5 w-5 text-[var(--gold)]" />
          <AlertTitle className="font-semibold text-[var(--fg1)]">
            <div className="flex items-center gap-2">
              {!hasMicrophone && <Mic className="h-4 w-4" />}
              {!hasSystemAudio && <Speaker className="h-4 w-4" />}
              {!hasMicrophone && !hasSystemAudio ? t('Permissions Required') : !hasMicrophone ? t('Microphone Permission Required') : t('System Audio Permission Required')}
            </div>
          </AlertTitle>
          {/* Action Buttons */}
          <div className="mt-4 flex flex-wrap gap-2">
            {isMacOS && !hasMicrophone && (
              <button
                onClick={openMicrophoneSettings}
                className="inline-flex items-center gap-2 rounded-full bg-[var(--gold)] px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] transition-colors hover:bg-[var(--gold-active)]"
              >
                <Mic className="h-4 w-4" />
                {t('Open Microphone Settings')}
              </button>
            )}
            {isMacOS && !hasSystemAudio && (
              <button
                onClick={openScreenRecordingSettings}
                className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] bg-[var(--gold)] hover:bg-[var(--gold-active)] rounded-md transition-colors"
              >
                <Speaker className="h-4 w-4" />
                {t('Open Screen Recording Settings')}
              </button>
            )}
            <button
              onClick={onRecheck}
              disabled={isRechecking}
              className="inline-flex items-center gap-2 rounded-full border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-4 py-2 text-sm font-medium text-[var(--fg1)] transition-colors hover:bg-[var(--state-hover-bg)] disabled:opacity-50"
            >
              <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
              {t('Recheck')}
            </button>
          </div>
          <AlertDescription className="mt-2 text-[var(--fg2)]">
            {/* Microphone Warning */}
            {!hasMicrophone && (
              <>
                <p className="mb-3">
                  {t('Meetily needs access to your microphone to record meetings. No microphone devices were detected.')}
                </p>
                <div className="space-y-2 text-sm mb-4">
                  <p className="font-medium">{t('Please check:')}</p>
                  <ul className="list-disc list-inside ml-2 space-y-1">
                    <li>{t('Your microphone is connected and powered on')}</li>
                    <li>{t('Microphone permission is granted in System Settings')}</li>
                    <li>{t('No other app is exclusively using the microphone')}</li>
                  </ul>
                </div>
              </>
            )}

            {/* System Audio Warning */}
            {!hasSystemAudio && (
              <>
                <p className="mb-3">
                  {hasMicrophone
                    ? t('System audio capture is not available. You can still record with your microphone, but computer audio won\'t be captured.')
                    : t('System audio capture is also not available.')}
                </p>
                {isMacOS && (
                  <div className="space-y-2 text-sm mb-4">
                    <p className="font-medium">{t('To enable system audio on macOS:')}</p>
                    <ul className="list-disc list-inside ml-2 space-y-1">
                      <li>{t('Install a virtual audio device (e.g., BlackHole 2ch)')}</li>
                      <li>{t('Grant Screen Recording permission to Meetily')}</li>
                      <li>{t('Configure your audio routing in Audio MIDI Setup')}</li>
                    </ul>
                  </div>
                )}
              </>
            )}


          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
