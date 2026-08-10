import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Check } from '@/components/deslop-icons';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';
import {
  TypeAvatar,
  TypeCell,
  TypePicker,
  TypeSection,
  TypeSectionList,
  TypeStartView,
} from '../type/TypeOnboarding';
import { TypeMicrophoneIcon, TypeSystemAudioIcon } from '../type/icons';

export function PermissionsStep() {
  const t = useT();
  const {
    goNext,
    permissions,
    setPermissionStatus,
  } = useOnboarding();
  const [pending, setPending] = useState<'microphone' | 'systemAudio' | null>(null);

  useEffect(() => {
    invoke<boolean>('check_system_audio_permissions_command')
      .then((granted) => {
        if (granted) setPermissionStatus('systemAudio', 'authorized');
      })
      .catch(() => undefined);
  }, [setPermissionStatus]);

  const openSettings = async (preferencePane: string) => {
    try {
      await invoke('open_system_settings', { preferencePane });
    } catch (error) {
      console.error('[onboarding] could not open System Settings:', error);
      toast.error(t('Could not open System Settings'));
    }
  };

  const requestMicrophone = async () => {
    if (permissions.microphone === 'authorized' || pending) return;
    if (permissions.microphone === 'denied') {
      await openSettings('Privacy_Microphone');
      return;
    }
    setPending('microphone');
    try {
      const granted = await invoke<boolean>('trigger_microphone_permission');
      setPermissionStatus('microphone', granted ? 'authorized' : 'denied');
    } catch (error) {
      console.error('[onboarding] microphone permission failed:', error);
      setPermissionStatus('microphone', 'denied');
    } finally {
      setPending(null);
    }
  };

  const requestSystemAudio = async () => {
    if (permissions.systemAudio === 'authorized' || pending) return;
    if (permissions.systemAudio === 'denied') {
      await openSettings('Privacy_AudioCapture');
      return;
    }
    setPending('systemAudio');
    try {
      const granted = await invoke<boolean>('trigger_system_audio_permission_command');
      setPermissionStatus('systemAudio', granted ? 'authorized' : 'denied');
    } catch (error) {
      console.error('[onboarding] system audio permission failed:', error);
      setPermissionStatus('systemAudio', 'denied');
    } finally {
      setPending(null);
    }
  };

  const allGranted = permissions.microphone === 'authorized' && permissions.systemAudio === 'authorized';

  useEffect(() => {
    if (allGranted && !pending) goNext();
  }, [allGranted, goNext, pending]);

  return (
    <>
      <TypeStartView
        title={t('Grant Permissions')}
        description={t('Memento needs access to your microphone and system audio to record meetings')}
      />
      <TypeSectionList>
        <TypeSection surface="primary-5">
          <TypeCell
            start={<TypeAvatar color={3}><TypeMicrophoneIcon /></TypeAvatar>}
            title={t('Microphone')}
            description={permissions.microphone === 'authorized' ? t('Access Granted') : t('Required to capture your voice during meetings')}
            end={(
              <TypePicker iconOnly={permissions.microphone === 'authorized'}>
                {permissions.microphone === 'authorized' ? <Check size={18} weight={600} /> : t('Enable')}
              </TypePicker>
            )}
            onClick={permissions.microphone === 'authorized' ? undefined : requestMicrophone}
            disabled={Boolean(pending)}
          />
          <TypeCell
            start={<TypeAvatar color={5}><TypeSystemAudioIcon /></TypeAvatar>}
            title={t('System Audio')}
            description={permissions.systemAudio === 'authorized' ? t('Access Granted') : t('Click Enable to grant Audio Capture permission')}
            end={(
              <TypePicker iconOnly={permissions.systemAudio === 'authorized'}>
                {permissions.systemAudio === 'authorized' ? <Check size={18} weight={600} /> : t('Enable')}
              </TypePicker>
            )}
            onClick={permissions.systemAudio === 'authorized' ? undefined : requestSystemAudio}
            disabled={Boolean(pending)}
          />
        </TypeSection>
      </TypeSectionList>
    </>
  );
}
