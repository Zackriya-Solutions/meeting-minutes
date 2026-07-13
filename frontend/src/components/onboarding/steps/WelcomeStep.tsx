import React from 'react';
import { Button } from '@/components/memento/Button';
import { Icon, MementoIconName } from '@/components/memento/Icon';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function WelcomeStep() {
  const { goNext } = useOnboarding();

  const features = [
    {
      icon: 'check' as MementoIconName,
      title: 'Данные остаются на твоём устройстве',
    },
    {
      icon: 'spark' as MementoIconName,
      title: 'Суть, выводы и задачи после встречи',
    },
    {
      icon: 'library' as MementoIconName,
      title: 'Локальная работа без обязательного облака',
    },
  ];

  return (
    <OnboardingContainer
      title="Встречи остаются с тобой"
      description="Записывай, расшифровывай и сохраняй суть на своём устройстве"
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Divider */}
        <div className="h-px w-16 bg-[var(--border-strong)]" />

        {/* Features Card */}
        <div className="mm-card w-full max-w-md space-y-4 p-6">
          {features.map((feature, index) => {
            return (
              <div key={index} className="flex items-start gap-3">
                <div className="flex-shrink-0 mt-0.5">
                  <div className="flex h-6 w-6 items-center justify-center rounded-full bg-[var(--gold-soft)] text-[var(--gold)]">
                    <Icon name={feature.icon} size={14} />
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
            className="w-full"
          >
            Начать настройку
          </Button>
          <p className="text-center text-xs text-gray-500">Займёт меньше трёх минут</p>
        </div>
      </div>
    </OnboardingContainer>
  );
}
