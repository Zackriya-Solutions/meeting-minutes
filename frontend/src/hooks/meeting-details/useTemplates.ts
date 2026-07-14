import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';

export function useTemplates() {
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
      description: `${t('Using')} "${templateName}" ${t('template for summary generation')}`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, []);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
  };
}
