import React from 'react';
import { AlertTriangle, Mic, Speaker, Settings, RefreshCw } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';

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
  // Don't show if both permissions are granted
  if (hasMicrophone && hasSystemAudio) {
    return null;
  }

  const isMacOS = navigator.userAgent.includes('Mac');

  const openSystemSettings = () => {
    if (isMacOS) {
      // On macOS, guide users to System Settings
      const settingsUrl = hasMicrophone
        ? 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture'
        : 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone';

      window.open(settingsUrl, '_blank');
    }
  };

  return (
    <div className="mb-4 space-y-3">
      {/* Microphone Permission Warning */}
      {!hasMicrophone && (
        <Alert variant="destructive" className="border-amber-400 bg-amber-50">
          <AlertTriangle className="h-5 w-5 text-amber-600" />
          <AlertTitle className="text-amber-900 font-semibold">
            <div className="flex items-center gap-2">
              <Mic className="h-4 w-4" />
              Microphone Permission Required
            </div>
          </AlertTitle>
          <AlertDescription className="text-amber-800 mt-2">
            <p className="mb-3">
              Meetily needs access to your microphone to record meetings. No microphone devices were detected.
            </p>
            <div className="space-y-2 text-sm">
              <p className="font-medium">Please check:</p>
              <ul className="list-disc list-inside ml-2 space-y-1">
                <li>Your microphone is connected and powered on</li>
                <li>Microphone permission is granted in System Settings</li>
                <li>No other app is exclusively using the microphone</li>
              </ul>
            </div>
            <div className="mt-4 flex gap-2">
              {isMacOS && (
                <button
                  onClick={openSystemSettings}
                  className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-amber-600 hover:bg-amber-700 rounded-md transition-colors"
                >
                  <Settings className="h-4 w-4" />
                  Open System Settings
                </button>
              )}
              <button
                onClick={onRecheck}
                disabled={isRechecking}
                className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-amber-900 bg-amber-100 hover:bg-amber-200 rounded-md transition-colors disabled:opacity-50"
              >
                <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
                Recheck
              </button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* System Audio Permission Warning */}
      {hasMicrophone && !hasSystemAudio && (
        <Alert variant="default" className="border-blue-400 bg-blue-50">
          <AlertTriangle className="h-5 w-5 text-blue-600" />
          <AlertTitle className="text-blue-900 font-semibold">
            <div className="flex items-center gap-2">
              <Speaker className="h-4 w-4" />
              System Audio Not Available
            </div>
          </AlertTitle>
          <AlertDescription className="text-blue-800 mt-2">
            <p className="mb-3">
              System audio capture is not available. You can still record with your microphone, but computer audio won't be captured.
            </p>
            {isMacOS && (
              <div className="space-y-2 text-sm">
                <p className="font-medium">To enable system audio on macOS:</p>
                <ul className="list-disc list-inside ml-2 space-y-1">
                  <li>Install a virtual audio device (e.g., BlackHole 2ch)</li>
                  <li>Grant Screen Recording permission to Meetily</li>
                  <li>Configure your audio routing in Audio MIDI Setup</li>
                </ul>
              </div>
            )}
            <div className="mt-4 flex gap-2">
              {isMacOS && (
                <button
                  onClick={openSystemSettings}
                  className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
                >
                  <Settings className="h-4 w-4" />
                  Open System Settings
                </button>
              )}
              <button
                onClick={onRecheck}
                disabled={isRechecking}
                className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-blue-900 bg-blue-100 hover:bg-blue-200 rounded-md transition-colors disabled:opacity-50"
              >
                <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
                Recheck
              </button>
            </div>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
