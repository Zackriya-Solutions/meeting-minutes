import React from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { Checklist } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import {
  TypeAvatar,
  TypeButtonRow,
  TypeCell,
  TypeLogoHeader,
  TypeSection,
  TypeSectionList,
  TypeStartView,
} from '../type/TypeOnboarding';
import { Button } from '@/components/ui/fluid-functional-button';
import { ShapeProvider } from '@/lib/shape-context';
import {
  TypeEfficiencyIcon,
  TypeLightbulbIcon,
  TypeSearchIcon,
} from '../type/icons';

/** Product introduction assembled from the same StartView, SectionList, Cell and button as Type. */
export function WelcomeStep() {
  const { goNext, startModelPack } = useOnboarding();
  const t = useT();

  const beginSetup = () => {
    void startModelPack();
    goNext();
  };

  return (
    <>
      <TypeLogoHeader />
      <TypeStartView
        title={t('Memento helps before, during, and after meetings')}
      />
      <TypeSectionList>
        <TypeSection surface="primary-5">
          <TypeCell
            start={<TypeAvatar color={3}><Checklist size={24} weight={600} /></TypeAvatar>}
            title={t('Prepares the agenda')}
            description={t('Collects context and questions for the next meeting')}
          />
          <TypeCell
            start={<TypeAvatar color={6}><TypeLightbulbIcon /></TypeAvatar>}
            title={t('Helps you catch up if distracted')}
            description={t('A short summary shows what you missed')}
          />
          <TypeCell
            start={<TypeAvatar color={4}><TypeSearchIcon /></TypeAvatar>}
            title={t('Answers questions about any meeting')}
            description={t('Finds the details you need in conversation history')}
          />
          <TypeCell
            start={<TypeAvatar color={0}><TypeEfficiencyIcon /></TypeAvatar>}
            title={t('Helps improve meetings')}
            description={t('Finds ineffective meetings and suggests how to improve them')}
          />
        </TypeSection>
      </TypeSectionList>
      <TypeButtonRow>
        <ShapeProvider defaultShape="rounded" publishToRoot={false}>
          <Button
            type="button"
            variant="primary"
            className="w-full text-[var(--black)] [--background:var(--background-primary)] [--foreground:var(--accent-orange)]"
            onClick={beginSetup}
          >
            {t('Get Started')}
          </Button>
        </ShapeProvider>
      </TypeButtonRow>
    </>
  );
}
