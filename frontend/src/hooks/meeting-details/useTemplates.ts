import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

type TemplateInfo = {
  id: string;
  name: string;
  description: string;
};

export function useTemplates() {
  const [availableTemplates, setAvailableTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>('standard_meeting');

  const fetchTemplates = useCallback(async () => {
    try {
      const templates = await invokeTauri('api_list_templates') as TemplateInfo[];
      console.log('Available templates:', templates);
      setAvailableTemplates(templates);
    } catch (error) {
      console.error('Failed to fetch templates:', error);
    }
  }, []);

  // Fetch available templates on mount
  useEffect(() => {
    fetchTemplates();
  }, [fetchTemplates]);

  // The template editor in Settings emits this after a save or delete, so the
  // picker stays current without an app restart.
  useEffect(() => {
    const unlisten = listen('templates-changed', () => {
      fetchTemplates();
    });

    return () => {
      unlisten.then((off) => off()).catch((error) =>
        console.warn('Failed to detach templates-changed listener:', error)
      );
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
