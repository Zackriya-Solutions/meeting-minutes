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

export type MeetingMemoryConfig = {
  meeting_id: string;
  memory_type: 'general' | 'standup' | 'interview';
  sensitivity: 'standard' | 'sensitive';
};

function templateForMemoryType(memoryType: MeetingMemoryConfig['memory_type']): string {
  if (memoryType === 'standup') return 'daily_standup';
  if (memoryType === 'interview') return 'interview_memory';
  return 'standard_meeting';
}

function memoryConfigForTemplate(templateId: string): Pick<MeetingMemoryConfig, 'memory_type' | 'sensitivity'> {
  if (templateId === 'daily_standup') {
    return { memory_type: 'standup', sensitivity: 'standard' };
  }
  if (templateId === 'interview_memory') {
    return { memory_type: 'interview', sensitivity: 'sensitive' };
  }
  return { memory_type: 'general', sensitivity: 'standard' };
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
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
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

    setSelectedTemplate(templateId);
    if (templateId === 'daily_standup' || templateId === 'interview_memory') {
      setTemplateSuggestion(null);
      if (meetingId) toast.dismiss(`standup-template-suggestion-${meetingId}`);
    }
    if (meetingId) {
      setMemoryConfig({ meeting_id: meetingId, ...nextMemory });
      invokeTauri<MeetingMemoryConfig>('api_set_meeting_memory_config', {
        meetingId,
        memoryType: nextMemory.memory_type,
        sensitivity: nextMemory.sensitivity,
      }).catch((error) => console.error('Failed to persist Memento memory type:', error));
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
          setSelectedTemplate(templateForMemoryType(config.memory_type));
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
        const description = explanation || t('Local signals suggest Standup V2. Confirm before generating.');
        setTemplateSuggestion({
          templateId: suggestion.templateId,
          title: t('Standup template suggested'),
          description,
        });
        toast.info(t('Standup template suggested'), {
          id: toastId,
          description,
          duration: Infinity,
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
    memoryConfig,
    templateSuggestion,
    dismissTemplateSuggestion: () => {
      setTemplateSuggestion(null);
      if (meetingId) toast.dismiss(`standup-template-suggestion-${meetingId}`);
    },
    handleTemplateSelection,
  };
}
