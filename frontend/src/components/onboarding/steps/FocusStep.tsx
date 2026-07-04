import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Users, Stethoscope, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import type { ProductFocus } from '@/types/onboarding';

const CLINICAL_SUMMARY_MODEL = 'medgemma:4b';
const CLINICAL_TEMPLATE_ID = 'psychatric_session';
const HAI_DEF_TERMS_URL = 'https://developers.google.com/health-ai-developer-foundations/terms';

export function FocusStep() {
  const {
    goNext,
    setProductFocus,
    setSelectedSummaryModel,
    selectedSummaryModel,
    recommendedSummaryModel,
  } = useOnboarding();
  const [choice, setChoice] = useState<ProductFocus>('general');
  const [disclaimerAccepted, setDisclaimerAccepted] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isMac, setIsMac] = useState(false);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  const options: Array<{
    value: ProductFocus;
    icon: React.ElementType;
    title: string;
    description: string;
  }> = [
    {
      value: 'general',
      icon: Users,
      title: 'General',
      description: 'Meetings, standups, and everyday notes.',
    },
    {
      value: 'clinician',
      icon: Stethoscope,
      title: 'Clinician-focused',
      description:
        'Clinical session notes with a SOAP template and the MedGemma 4B clinical model.',
    },
  ];

  const canContinue = choice === 'general' || disclaimerAccepted;

  const handleContinue = async () => {
    if (!canContinue || isSaving) return;

    setIsSaving(true);
    try {
      setProductFocus(choice);
      await invoke('set_product_focus', { focus: choice });

      if (choice === 'clinician') {
        setSelectedSummaryModel(CLINICAL_SUMMARY_MODEL);
        await invoke('set_default_summary_template', { templateId: CLINICAL_TEMPLATE_ID });
      } else if (selectedSummaryModel === CLINICAL_SUMMARY_MODEL) {
        // User came back and switched from clinician to general - undo the clinical defaults
        if (recommendedSummaryModel) {
          setSelectedSummaryModel(recommendedSummaryModel);
        }
        await invoke('set_default_summary_template', { templateId: 'standard_meeting' });
      }

      goNext();
    } catch (error) {
      console.error('[FocusStep] Failed to save product focus:', error);
      // Continue anyway - the debounced onboarding auto-save will retry persistence
      goNext();
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <OnboardingContainer
      title="Choose your focus"
      description="Pick how you'll use Meetily. This selects the AI model and note template that fit best."
      step={3}
      totalSteps={isMac ? 5 : 4}
    >
      <div className="flex flex-col items-center space-y-6">
        {/* Focus Option Cards */}
        <div className="w-full max-w-lg space-y-3">
          {options.map((option) => {
            const Icon = option.icon;
            const isSelected = choice === option.value;
            return (
              <button
                key={option.value}
                type="button"
                onClick={() => setChoice(option.value)}
                className={`w-full text-left bg-white rounded-xl border p-5 transition-colors ${
                  isSelected
                    ? 'border-gray-900 ring-1 ring-gray-900'
                    : 'border-gray-200 hover:border-gray-400'
                }`}
              >
                <div className="flex items-start gap-4">
                  <div className="w-10 h-10 rounded-full bg-gray-100 flex items-center justify-center flex-shrink-0">
                    <Icon className="w-5 h-5 text-gray-600" />
                  </div>
                  <div>
                    <h3 className="font-medium text-gray-900">{option.title}</h3>
                    <p className="text-sm text-gray-500 mt-1">{option.description}</p>
                  </div>
                </div>
              </button>
            );
          })}
        </div>

        {/* Clinical Disclaimer */}
        {choice === 'clinician' && (
          <div className="w-full max-w-lg bg-gray-100 rounded-lg p-4 text-sm text-gray-800 space-y-3">
            <p>
              Summaries are AI-generated and may contain errors — always review them before
              clinical use. Meetily is not a medical device and does not provide medical advice
              or diagnosis. The MedGemma model is provided under and subject to{' '}
              <a
                href={HAI_DEF_TERMS_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="underline hover:text-gray-900"
              >
                Google&apos;s Health AI Developer Foundations terms
              </a>
              .
            </p>
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={disclaimerAccepted}
                onChange={(e) => setDisclaimerAccepted(e.target.checked)}
                className="mt-0.5"
              />
              <span>I understand and agree to these terms.</span>
            </label>
          </div>
        )}

        {/* Continue Button */}
        <div className="w-full max-w-xs">
          <Button
            onClick={handleContinue}
            disabled={!canContinue || isSaving}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isSaving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : 'Continue'}
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
