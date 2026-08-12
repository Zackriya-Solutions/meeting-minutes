import React, { useEffect } from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useFinishOnboarding } from '../useFinishOnboarding';
import { useT } from '@/lib/i18n';
import {
  TypeButtonRow,
  TypeExitCover,
  TypeStartView,
} from '../type/TypeOnboarding';
import { RegularButton } from '@/vendor/deslop/mini-app/components/Button';

export function ReadyStep() {
  const t = useT();
  const { modelPack, startModelPack } = useOnboarding();
  const { finish, isFinishing } = useFinishOnboarding();

  useEffect(() => {
    if (modelPack.status === 'idle') void startModelPack();
  }, [modelPack.status, startModelPack]);

  const ready = modelPack.status === 'ready';
  const failed = modelPack.status === 'error';
  const title = ready
    ? t('Memento is ready to work')
    : failed
      ? t('Setup needs attention')
      : t('Preparing Memento');
  const description = ready
    ? t('Everything is set up. You can record your first meeting.')
    : failed
      ? t('The recognition models could not be prepared. Try again.')
      : t('Finishing local speech and speaker recognition setup.');

  return (
    <>
      <TypeStartView title={title} description={description} />
      <TypeButtonRow>
        <RegularButton
          variant="filled"
          label={isFinishing ? t('Checking...') : t('Start using Memento')}
          isFill
          aria-disabled={!ready || isFinishing}
          style={!ready || isFinishing ? { opacity: 0.45, pointerEvents: 'none' } : undefined}
          onClick={ready && !isFinishing ? () => void finish() : undefined}
        />
      </TypeButtonRow>
      {isFinishing ? <TypeExitCover /> : null}
    </>
  );
}
