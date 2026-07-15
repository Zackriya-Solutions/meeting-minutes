import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';

type TemplateSuggestion = {
  templateId: string;
  confidence: 'low' | 'medium' | 'high';
  score: number;
  reasons: string[];
};

const suggestionReasonKeys: Record<string, string> = {
  standup_title: 'standup-like title',
  reviewed_series_history: 'reviewed standups in this series',
  status_round_language: 'status-round language',
  standup_time_window: 'usual standup time',
  standup_duration: 'standup-like duration',
};

export function useTemplates(meetingId?: string) {
  const t = useT();
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>('standard_meeting');

  // Fetch available templates on mount
  useEffect(() => {
    const fetchTemplates = async () => {
      try {
        const templates = await invokeTauri('api_list_templates') as Array<{
          id: string;
          name: string;
          description: string;
        }>;
        console.log('Available templates:', templates);
        setAvailableTemplates(templates);
      } catch (error) {
        console.error('Failed to fetch templates:', error);
      }
    };
    fetchTemplates();
  }, []);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success(t('Template selected'), {
      description: `${t('Using')} "${t(templateName)}" ${t('template for summary generation')}`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, [t]);

  useEffect(() => {
    setSelectedTemplate('standard_meeting');
  }, [meetingId]);

  useEffect(() => {
    if (!meetingId) return;

    let cancelled = false;
    const toastId = `standup-template-suggestion-${meetingId}`;
    invokeTauri<TemplateSuggestion>('suggest_summary_template', { meetingId })
      .then((suggestion) => {
        if (cancelled || suggestion.templateId !== 'daily_standup' || suggestion.confidence === 'low') {
          return;
        }
        const explanation = suggestion.reasons
          .map((reason) => suggestionReasonKeys[reason])
          .filter((reason): reason is string => Boolean(reason))
          .map((reason) => t(reason))
          .join(' · ');
        toast.info(t('Standup template suggested'), {
          id: toastId,
          description: explanation || t('Local signals suggest Standup V2. Confirm before generating.'),
          duration: 12000,
          action: {
            label: t('Use Standup V2'),
            onClick: () => handleTemplateSelection('daily_standup', 'Daily Standup'),
          },
        });
      })
      .catch((error) => console.warn('Could not suggest a summary template:', error));
    return () => {
      cancelled = true;
      toast.dismiss(toastId);
    };
  }, [handleTemplateSelection, meetingId, t]);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
  };
}
