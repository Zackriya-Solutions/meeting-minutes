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
  confirmationRequired: boolean;
};

export type MeetingMemoryConfig = {
  meeting_id: string;
  memory_type: 'general' | 'standup' | 'interview';
  sensitivity: 'standard' | 'sensitive';
  summary_template_id: string;
};

function templateForMemoryConfig(config: MeetingMemoryConfig): string {
  if (config.summary_template_id) return config.summary_template_id;
  if (config.memory_type === 'standup') return 'daily_standup';
  if (config.memory_type === 'interview') return 'interview_memory';
  return 'standard_meeting';
}

function memoryConfigForTemplate(templateId: string): Pick<MeetingMemoryConfig, 'memory_type' | 'sensitivity' | 'summary_template_id'> {
  if (templateId === 'daily_standup') {
    return { memory_type: 'standup', sensitivity: 'standard', summary_template_id: templateId };
  }
  if (templateId === 'interview_memory') {
    return { memory_type: 'interview', sensitivity: 'sensitive', summary_template_id: templateId };
  }
  if (templateId === 'one_on_one') {
    return { memory_type: 'general', sensitivity: 'sensitive', summary_template_id: templateId };
  }
  return { memory_type: 'general', sensitivity: 'standard', summary_template_id: templateId };
}

export type VisibleTemplateSuggestion = {
  templateId: string;
  title: string;
  description: string;
};
const suggestionReasonKeys: Record<string, string> = {
  standup_title: 'standup-like title',
  reviewed_series_history: 'reviewed standups in this series',
  status_round_language: 'status-round language',
  status_round_handoff: 'participant hand-offs',
  standup_time_window: 'usual standup time',
  standup_duration: 'standup-like duration',
  one_on_one_title: 'one-on-one title',
  one_on_one_content: 'check-in, feedback, support, growth, or prior follow-up language',
  reviewed_one_on_one_history: 'reviewed one-on-ones for this confirmed pair',
};

export function useTemplates(meetingId?: string) {
  const t = useT();
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>('standard_meeting');
  const [memoryConfig, setMemoryConfig] = useState<MeetingMemoryConfig | null>(null);
  const [templateSuggestion, setTemplateSuggestion] = useState<VisibleTemplateSuggestion | null>(null);

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
  const handleTemplateSelection = useCallback(async (templateId: string, templateName: string) => {
    const nextMemory = memoryConfigForTemplate(templateId);
    const leavingPrivateMemory = Boolean(memoryConfig)
      && (memoryConfig!.memory_type === 'interview' || memoryConfig!.sensitivity === 'sensitive')
      && nextMemory.memory_type !== 'interview'
      && nextMemory.sensitivity !== 'sensitive';
    if (leavingPrivateMemory && !window.confirm(t(
      'Switching from a sensitive memory restores cloud processing and search indexing defaults. Continue?'
    ))) {
      return;
    }

    if (meetingId) {
      try {
        const persisted = await invokeTauri<MeetingMemoryConfig>('api_set_meeting_memory_config', {
          meetingId,
          memoryType: nextMemory.memory_type,
          sensitivity: nextMemory.sensitivity,
          summaryTemplateId: nextMemory.summary_template_id,
        });
        setMemoryConfig(persisted);
      } catch (error) {
        console.error('Failed to persist Memento memory type:', error);
        toast.error(t('Failed to select template'), { description: String(error) });
        return;
      }
    }
    setSelectedTemplate(templateId);
    if (templateId === 'daily_standup' || templateId === 'interview_memory' || templateId === 'one_on_one') {
      setTemplateSuggestion(null);
      if (meetingId) toast.dismiss(`memory-template-suggestion-${meetingId}`);
    }
    toast.success(t('Template selected'), {
      description: `${t('Using')} "${t(templateName)}" ${t('template for summary generation')}`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, [meetingId, memoryConfig, t]);

  useEffect(() => {
    setSelectedTemplate('standard_meeting');
    setMemoryConfig(null);
    setTemplateSuggestion(null);
    if (!meetingId) return;

    let cancelled = false;
    invokeTauri<MeetingMemoryConfig>('api_get_meeting_memory_config', { meetingId })
      .then((config) => {
        if (!cancelled) {
          setMemoryConfig(config);
          setSelectedTemplate(templateForMemoryConfig(config));
        }
      })
      .catch((error) => console.warn('Could not restore Memento memory type:', error));
    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  useEffect(() => {
    if (!meetingId) return;

    let cancelled = false;
    const toastId = `memory-template-suggestion-${meetingId}`;
    invokeTauri<TemplateSuggestion>('suggest_summary_template', { meetingId })
      .then((suggestion) => {
        if (cancelled || !['daily_standup', 'one_on_one'].includes(suggestion.templateId) || suggestion.confidence === 'low') {
          return;
        }
        const explanation = suggestion.reasons
          .map((reason) => suggestionReasonKeys[reason])
          .filter((reason): reason is string => Boolean(reason))
          .map((reason) => t(reason))
          .join(' · ');
        const oneOnOne = suggestion.templateId === 'one_on_one';
        const title = oneOnOne ? t('One-on-one template suggested') : t('Standup template suggested');
        const description = explanation || t(oneOnOne
          ? 'Local signals suggest a one-on-one meeting template. Confirm before generating.'
          : 'Local signals suggest a daily standup template. Confirm before generating.');
        setTemplateSuggestion({
          templateId: suggestion.templateId,
          title,
          description,
        });
        toast.info(title, {
          id: toastId,
          description,
          duration: Infinity,
          action: {
            label: t(oneOnOne ? 'Use One-on-One' : 'Use Daily Standup'),
            onClick: () => handleTemplateSelection(
              oneOnOne ? 'one_on_one' : 'daily_standup',
              oneOnOne ? 'One-on-One' : 'Daily Standup',
            ),
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
    memoryConfig,
    templateSuggestion,
    dismissTemplateSuggestion: () => {
      setTemplateSuggestion(null);
      if (meetingId) toast.dismiss(`memory-template-suggestion-${meetingId}`);
    },
    handleTemplateSelection,
  };
}
