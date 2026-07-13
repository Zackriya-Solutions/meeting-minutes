import React from 'react';
import { AlertTriangle, Mic, Speaker, RefreshCw } from '@/components/memento/LucideCompat';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { invoke } from '@tauri-apps/api/core';
import { useIsLinux } from '@/hooks/usePlatform';

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
              {!hasMicrophone && !hasSystemAudio ? 'Нужны разрешения' : !hasMicrophone ? 'Нужен доступ к микрофону' : 'Нужен доступ к системному звуку'}
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
                Открыть настройки микрофона
              </button>
            )}
            {isMacOS && !hasSystemAudio && (
              <button
                onClick={openScreenRecordingSettings}
                className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] bg-[var(--gold)] hover:bg-[var(--gold)] rounded-md transition-colors"
              >
                <Speaker className="h-4 w-4" />
                Открыть настройки записи экрана
              </button>
            )}
            <button
              onClick={onRecheck}
              disabled={isRechecking}
              className="inline-flex items-center gap-2 rounded-full border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-4 py-2 text-sm font-medium text-[var(--fg1)] transition-colors hover:bg-[var(--state-hover-bg)] disabled:opacity-50"
            >
              <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
              Проверить снова
            </button>
          </div>
          <AlertDescription className="mt-2 text-[var(--fg2)]">
            {/* Microphone Warning */}
            {!hasMicrophone && (
              <>
                <p className="mb-3">
                  Memento нужен доступ к микрофону для записи встреч. Микрофон не обнаружен.
                </p>
                <div className="space-y-2 text-sm mb-4">
                  <p className="font-medium">Проверь:</p>
                  <ul className="list-disc list-inside ml-2 space-y-1">
                    <li>Микрофон подключён и включён</li>
                    <li>Доступ к микрофону разрешён в системных настройках</li>
                    <li>Другое приложение не заняло микрофон</li>
                  </ul>
                </div>
              </>
            )}

            {/* System Audio Warning */}
            {!hasSystemAudio && (
              <>
                <p className="mb-3">
                  {hasMicrophone
                    ? 'Системный звук недоступен. Можно записывать с микрофона, но звук компьютера не попадёт в запись.'
                    : 'Системный звук тоже недоступен.'}
                </p>
                {isMacOS && (
                  <div className="space-y-2 text-sm mb-4">
                    <p className="font-medium">Чтобы включить системный звук в macOS:</p>
                    <ul className="list-disc list-inside ml-2 space-y-1">
                      <li>Установи виртуальное аудиоустройство, например BlackHole 2ch</li>
                      <li>Разреши Memento запись экрана</li>
                      <li>Настрой маршрутизацию в Audio MIDI Setup</li>
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
