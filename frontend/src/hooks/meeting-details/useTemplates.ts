import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

export function useTemplates() {
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>('standard_meeting');

  const fetchTemplates = useCallback(async () => {
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
  }, []);

  // Fetch on mount
  useEffect(() => {
    fetchTemplates();
  }, [fetchTemplates]);

  // Refresh when templates change (e.g. user created/deleted one in Settings)
  useEffect(() => {
    const unlisten = listen('templates-changed', () => {
      fetchTemplates();
    });
    return () => {
      unlisten.then(fn => fn());
    };
  }, [fetchTemplates]);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success('Template selected', {
      description: `Using "${templateName}" template for summary generation`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, []);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
  };
}
