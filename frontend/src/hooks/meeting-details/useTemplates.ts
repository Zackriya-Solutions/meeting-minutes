import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
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

  // Fetch available templates on mount
  useEffect(() => {
    fetchTemplates();
  }, [fetchTemplates]);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success('Template selected', {
      description: `Using "${templateName}" template for summary generation`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, []);

  const getRawTemplate = useCallback(async (templateId: string) => {
    try {
      return await invokeTauri('api_get_raw_template', { templateId }) as string;
    } catch (error) {
      console.error('Failed to get raw template:', error);
      throw error;
    }
  }, []);

  const saveTemplate = useCallback(async (templateId: string, templateJson: string) => {
    try {
      await invokeTauri('api_save_template', { templateId, templateJson });
      toast.success('Template saved successfully');
      await fetchTemplates(); // Refresh the list in case metadata changed
    } catch (error) {
      console.error('Failed to save template:', error);
      toast.error('Failed to save template', {
        description: String(error)
      });
      throw error;
    }
  }, [fetchTemplates]);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
    refreshTemplates: fetchTemplates,
    getRawTemplate,
    saveTemplate
  };
}
