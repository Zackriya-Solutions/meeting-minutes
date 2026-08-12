import React, { useEffect } from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useFinishOnboarding } from '../useFinishOnboarding';
import { useT } from '@/lib/i18n';
import {
  TypeButtonRow,
  TypeExitCover,
  TypeStartView,
} from '../type/TypeOnboarding';
import { Button } from '@/components/ui/fluid-functional-button';
import { ShapeProvider } from '@/lib/shape-context';

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
        <ShapeProvider defaultShape="rounded" publishToRoot={false}>
          <Button
            type="button"
            variant="primary"
            className="w-full text-[var(--black)] [--background:var(--background-primary)] [--foreground:var(--accent-orange)]"
            loading={isFinishing}
            disabled={!ready || isFinishing}
            onClick={() => void finish()}
          >
            {isFinishing ? t('Checking...') : t('Start using Memento')}
          </Button>
        </ShapeProvider>
      </TypeButtonRow>
      {isFinishing ? <TypeExitCover /> : null}
    </>
  );
}
