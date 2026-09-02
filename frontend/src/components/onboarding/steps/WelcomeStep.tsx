import React from 'react';
import { Lock, Sparkles, Cpu } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function WelcomeStep() {
  const { goNext } = useOnboarding();

  const features = [
    {
      icon: Lock,
      title: 'Your data never leaves your device',
    },
    {
      icon: Sparkles,
      title: 'Intelligent summaries & insights',
    },
    {
      icon: Cpu,
      title: 'Works offline, no cloud required',
    },
  ];

  return (
    <OnboardingContainer
      title="Welcome to PulseTalq"
      description="Capture your voice, keep it private, and turn every transcript into useful work."
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-start space-y-8">
        {/* Divider */}
        <div className="w-24 h-0.5 bg-[var(--pt-accent)]" />

        {/* Features Card */}
        <div className="w-full bg-[var(--pt-surface)] rounded-[3px] border border-[var(--pt-border)] shadow-[0_12px_28px_rgba(11,11,12,.07)] p-6 grid md:grid-cols-3 gap-6">
          {features.map((feature, index) => {
            const Icon = feature.icon;
            return (
              <div key={index} className="flex flex-col items-start gap-4 border-t-2 border-[var(--pt-text)] pt-4">
                <div className="flex-shrink-0 mt-0.5">
                  <div className="w-8 h-8 border border-[var(--pt-border-strong)] flex items-center justify-center">
                    <Icon className="w-4 h-4 text-[var(--pt-text)]" />
                  </div>
                </div>
                <p className="text-sm text-[var(--pt-text-secondary)] leading-relaxed">{feature.title}</p>
              </div>
            );
          })}
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-sm space-y-3">
          <Button
            onClick={goNext}
            className="w-full h-11 rounded-[3px] bg-[var(--pt-text)] hover:bg-[var(--pt-surface-dark)] text-[var(--pt-text-inverse)] focus-visible:ring-[var(--pt-accent)] active:scale-[.99]"
          >
            Start setup
          </Button>
          <p className="text-xs text-left text-[var(--pt-text-tertiary)]">Takes less than 3 minutes. Audio stays on this device.</p>
        </div>
      </div>
    </OnboardingContainer>
  );
}
