import React from 'react';
import { Button } from '@/components/ui/button';
import { Icon, MementoIconName } from '@/components/memento/Icon';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';

export function WelcomeStep() {
  const { goNext } = useOnboarding();
  const t = useT();

  const features = [
    {
      icon: 'check' as MementoIconName,
      title: 'Your data never leaves your device',
    },
    {
      icon: 'spark' as MementoIconName,
      title: 'Intelligent summaries & insights',
    },
    {
      icon: 'library' as MementoIconName,
      title: 'Works offline, no cloud required',
    },
  ];

  return (
    <OnboardingContainer
      title={t('Welcome to Memento')}
      description={t('Record. Transcribe. Summarize. All on your device.')}
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Divider */}
        <div className="h-px w-16 bg-border" />

        {/* Features Card */}
        <div className="mm-card w-full max-w-md space-y-4 p-6">
          {features.map((feature, index) => {
            return (
              <div key={index} className="flex items-start gap-3">
                <div className="flex-shrink-0 mt-0.5">
                  <div className="flex h-6 w-6 items-center justify-center rounded-full bg-primary/10 text-primary">
                    <Icon name={feature.icon} size={14} />
                  </div>
                </div>
                <p className="text-sm text-muted-foreground leading-relaxed">{t(feature.title)}</p>
              </div>
            );
          })}
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-3">
          <Button
            onClick={goNext}
            className="w-full"
          >
            {t('Get Started')}
          </Button>
          <p className="text-center text-xs text-muted-foreground">{t('Takes less than 3 minutes')}</p>
        </div>
      </div>
    </OnboardingContainer>
  );
}
