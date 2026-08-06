import React from 'react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';

/**
 * First screen. It shows the product instead of describing it: two lines of a transcript and
 * the two entries they turn into. Everything before this was a list of promises ("умные
 * итоги", "работает офлайн") that means nothing until you have recorded something.
 */
export function WelcomeStep() {
  const { goNext } = useOnboarding();
  const t = useT();

  const replies = [
    { at: '00:42', speaker: 'Аня', text: t('Texts are on me, I will send them by end of day') },
    { at: '00:54', speaker: 'Игорь', text: t('Then I will put the build together on Thursday morning') },
  ];

  const outcome = [
    { kind: t('Decision'), text: t('Release on Friday the 21st') },
    { kind: t('Task'), text: t('Игорь — build on Thursday, before 12:00') },
  ];

  return (
    <OnboardingContainer
      title={t('A meeting turns into its outcome')}
      description={t('Memento listens to the call, writes down who said what, and collects the decisions, tasks and figures.')}
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-center space-y-8">
        <div className="mm-card w-full max-w-md space-y-4 p-5">
          <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            {t('What it looks like')}
          </p>

          <div className="space-y-2">
            {replies.map((reply) => (
              <p key={reply.at} className="text-sm leading-relaxed text-muted-foreground">
                <span className="tabular-nums text-xs text-muted-foreground/70">{reply.at}</span>{' '}
                <span className="font-medium text-foreground">{reply.speaker}:</span> {reply.text}
              </p>
            ))}
          </div>

          <div className="h-px bg-border" />

          <div className="space-y-2">
            {outcome.map((item) => (
              <p key={item.kind} className="text-sm leading-relaxed text-foreground">
                <span className="text-muted-foreground">{item.kind}:</span> {item.text}
              </p>
            ))}
          </div>
        </div>

        <div className="w-full max-w-xs space-y-3">
          <Button onClick={goNext} className="w-full">
            {t('Get Started')}
          </Button>
          <p className="text-center text-xs text-muted-foreground">
            {t('Setup takes about a minute. A ready example meeting opens at the end — you can delete it.')}
          </p>
        </div>
      </div>
    </OnboardingContainer>
  );
}
