import React, { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Icon } from '@/components/memento/Icon';
import { OnboardingContainer } from '../OnboardingContainer';
import { PermissionRow } from '../shared';
import { useFinishOnboarding } from '../useFinishOnboarding';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';

export function PermissionsStep() {
  const t = useT();
  const { setPermissionStatus, setPermissionsSkipped, permissions } = useOnboarding();
  const { finish, isFinishing } = useFinishOnboarding();
  const [isPending, setIsPending] = useState(false);

  // Check permissions - only logs current state, doesn't auto-authorize
  // Actual permission checks are done via explicit user actions (clicking Enable)
  const checkPermissions = useCallback(async () => {
    console.log('[PermissionsStep] Current permission states:');
    console.log(`  - Microphone: ${permissions.microphone}`);
    console.log(`  - System Audio: ${permissions.systemAudio}`);
    // Don't auto-set permissions based on device availability
    // Permissions should only be set after explicit user action via Enable button
  }, [permissions.microphone, permissions.systemAudio]);

  // Check permissions on mount
  useEffect(() => {
    checkPermissions();
  }, [checkPermissions]);

  /**
   * Open the System Settings pane for a permission the user has already denied.
   *
   * The pane anchor is required: `open_system_settings` takes it as an argument, and calling
   * the command without one made the button silently do nothing (the rejected invoke fell into
   * a catch whose `alert()` a Tauri webview never shows). If macOS does not recognise the
   * anchor it opens Privacy & Security itself, which is still where the user needs to be.
   */
  const openSettingsPane = async (
    pane: string,
    permission: 'microphone' | 'systemAudio',
  ) => {
    try {
      await invoke('open_system_settings', { preferencePane: pane });
    } catch (error) {
      console.error('[PermissionsStep] Could not open System Settings:', error);
      toast.error(t('Could not open System Settings'), {
        description:
          permission === 'microphone'
            ? t('Please enable microphone access in System Preferences > Security & Privacy > Microphone')
            : t('Please enable Audio Capture in System Settings → Privacy & Security → Audio Capture'),
      });
    }
  };

  // Request microphone permission
  const handleMicrophoneAction = async () => {
    if (permissions.microphone === 'denied') {
      await openSettingsPane('Privacy_Microphone', 'microphone');
      return;
    }

    setIsPending(true);
    try {
      console.log('[PermissionsStep] Triggering microphone permission...');
      const granted = await invoke<boolean>('trigger_microphone_permission');
      console.log('[PermissionsStep] Microphone permission result:', granted);

      if (granted) {
        setPermissionStatus('microphone', 'authorized');
      } else {
        // Permission was denied or dialog was dismissed
        setPermissionStatus('microphone', 'denied');
      }
    } catch (err) {
      console.error('[PermissionsStep] Failed to request microphone permission:', err);
      setPermissionStatus('microphone', 'denied');
    } finally {
      setIsPending(false);
    }
  };

  // Request system audio permission
  const handleSystemAudioAction = async () => {
    if (permissions.systemAudio === 'denied') {
      await openSettingsPane('Privacy_AudioCapture', 'systemAudio');
      return;
    }

    setIsPending(true);
    try {
      console.log('[PermissionsStep] Triggering Audio Capture permission...');
      // Backend creates Core Audio tap, captures audio, and verifies it's not silence
      // Returns true if permission granted and audio verified, false if denied (silence)
      const granted = await invoke<boolean>('trigger_system_audio_permission_command');
      console.log('[PermissionsStep] System audio permission result:', granted);

      if (granted) {
        setPermissionStatus('systemAudio', 'authorized');
        console.log('[PermissionsStep] Audio Capture permission verified - audio is not silence');
      } else {
        // Permission was denied (audio is silence)
        setPermissionStatus('systemAudio', 'denied');
        console.log('[PermissionsStep] Audio Capture permission denied - audio is silence');
      }
    } catch (err) {
      console.error('[PermissionsStep] Failed to request system audio permission:', err);
      setPermissionStatus('systemAudio', 'denied');
    } finally {
      setIsPending(false);
    }
  };

  const handleSkip = async () => {
    setPermissionsSkipped(true);
    await finish();
  };

  const allPermissionsGranted =
    permissions.microphone === 'authorized' &&
    permissions.systemAudio === 'authorized';

  return (
    <OnboardingContainer
      title={t('Grant Permissions')}
      description={t('Memento needs access to your microphone and system audio to record meetings')}
      step={3}
      totalSteps={3}
    >
      <div className="max-w-lg mx-auto space-y-6">
        {/* Permission Rows */}
        <div className="space-y-4">
          {/* Microphone */}
          <PermissionRow
            icon={<Icon name="mic" />}
            title={t('Microphone')}
            description={t('Required to capture your voice during meetings')}
            status={permissions.microphone}
            isPending={isPending}
            onAction={handleMicrophoneAction}
          />

          {/* System Audio */}
          <PermissionRow
            icon={<Icon name="wave" />}
            title={t('System Audio')}
            description={t('Click Enable to grant Audio Capture permission')}
            status={permissions.systemAudio}
            isPending={isPending}
            onAction={handleSystemAudioAction}
          />
        </div>

        {/* Action Buttons */}
        <div className="flex flex-col gap-3 pt-4">
          <Button
            onClick={() => void finish()}
            disabled={!allPermissionsGranted || isFinishing}
            className="w-full h-11"
          >
            {t('Finish Setup')}
          </Button>

          <button
            onClick={handleSkip}
            className="text-sm text-muted-foreground transition-colors hover:text-foreground"
          >
            {t("I'll do this later")}
          </button>

          {!allPermissionsGranted && (
            <p className="text-xs text-center text-muted-foreground">
              {t("Recording won't work without permissions. You can grant them later in settings.")}
            </p>
          )}
        </div>
      </div>
    </OnboardingContainer>
  );
}
