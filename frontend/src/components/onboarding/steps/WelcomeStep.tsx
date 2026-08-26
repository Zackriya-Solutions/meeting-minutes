import React from 'react';
import { useTranslation } from 'react-i18next';
import { Lock, Sparkles, Cpu } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function WelcomeStep() {
  const { t } = useTranslation();
  const { goNext } = useOnboarding();

  const features = [
    {
      icon: Lock,
      title: t('compliance.dataNeverLeaves'),
    },
    {
      icon: Sparkles,
      title: t('onboarding.welcomeSubtitle'),
    },
    {
      icon: Cpu,
      title: t('languagePicker.worksOffline'),
    },
  ];

  return (
    <OnboardingContainer
      title={t('onboarding.welcome')}
      description={t('onboarding.welcomeSubtitle')}
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Divider */}
        <div className="w-16 h-px bg-gray-300" />

        {/* Features Card */}
        <div className="w-full max-w-md bg-white rounded-lg border border-gray-200 shadow-sm p-6 space-y-4">
          {features.map((feature, index) => {
            const Icon = feature.icon;
            return (
              <div key={index} className="flex items-start gap-3">
                <div className="flex-shrink-0 mt-0.5">
                  <div className="w-5 h-5 rounded-full bg-gray-100 flex items-center justify-center">
                    <Icon className="w-3 h-3 text-gray-700" />
                  </div>
                </div>
                <p className="text-sm text-gray-700 leading-relaxed">{feature.title}</p>
              </div>
            );
          })}
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-3">
          <Button
            onClick={goNext}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white"
          >
            {t('onboarding.getStarted')}
          </Button>
          <p className="text-xs text-center text-gray-500">{t('onboarding.thisMayTakeAFewMinutes')}</p>
        </div>
      </div>
    </OnboardingContainer>
  );
}
