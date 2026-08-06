import React from 'react';
import { AlertTriangle, Mic, Speaker, RefreshCw } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useIsLinux } from '@/hooks/usePlatform';
import { Button } from '@/components/ui/button';

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
  isRechecking = false,
}: PermissionWarningProps) {
  const isLinux = useIsLinux();

  // Linux needs no permission grants, and a full grant needs no warning.
  if (isLinux || (hasMicrophone && hasSystemAudio)) return null;

  const isMacOS = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');

  const openPane = (preferencePane: string) => async () => {
    try {
      await invoke('open_system_settings', { preferencePane });
    } catch (error) {
      console.error(`Failed to open ${preferencePane}:`, error);
    }
  };

  // Missing mic blocks recording entirely; missing system audio only degrades
  // it. Different severities deserve different weight.
  const blocking = !hasMicrophone;

  const title = blocking
    ? hasSystemAudio
      ? 'Microphone access needed'
      : 'Audio access needed'
    : 'System audio unavailable';

  return (
    <div
      role="alert"
      className={
        blocking
          ? 'rounded-lg border border-danger/30 bg-danger-soft p-3.5'
          : 'rounded-lg border border-warn/30 bg-warn-soft p-3.5'
      }
    >
      <div className="flex gap-2.5">
        <AlertTriangle
          aria-hidden
          className={`mt-px h-4 w-4 shrink-0 ${blocking ? 'text-danger-ink' : 'text-warn-ink'}`}
        />
        <div className="min-w-0 flex-1">
          <p
            className={`text-base font-semibold ${blocking ? 'text-danger-ink' : 'text-warn-ink'}`}
          >
            {title}
          </p>

          <div className="mt-1.5 space-y-2.5 text-sm leading-relaxed text-ink-muted">
            {!hasMicrophone && (
              <div>
                <p>
                  No microphone was detected, so recording cannot start. Check that
                  it is connected, that Conversationaly has permission, and that no
                  other app is holding it exclusively.
                </p>
              </div>
            )}

            {!hasSystemAudio && (
              <div>
                <p>
                  {hasMicrophone
                    ? 'Your microphone works, but the other side of the call will not be captured.'
                    : 'Computer audio is also unavailable.'}
                </p>
                {isMacOS && (
                  <ul className="mt-1.5 list-inside list-disc space-y-0.5">
                    <li>Install a virtual audio device such as BlackHole 2ch</li>
                    <li>Grant Screen Recording permission to Conversationaly</li>
                    <li>Route your output through it in Audio MIDI Setup</li>
                  </ul>
                )}
              </div>
            )}
          </div>

          <div className="mt-3 flex flex-wrap gap-1.5">
            {isMacOS && !hasMicrophone && (
              <Button size="sm" onClick={openPane('Privacy_Microphone')}>
                <Mic aria-hidden />
                Microphone settings
              </Button>
            )}
            {isMacOS && !hasSystemAudio && (
              <Button
                size="sm"
                variant={hasMicrophone ? 'outline' : 'default'}
                onClick={openPane('Privacy_ScreenCapture')}
              >
                <Speaker aria-hidden />
                Screen recording settings
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={onRecheck} disabled={isRechecking}>
              <RefreshCw className={isRechecking ? 'animate-spin' : undefined} aria-hidden />
              {isRechecking ? 'Checking…' : 'Check again'}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
