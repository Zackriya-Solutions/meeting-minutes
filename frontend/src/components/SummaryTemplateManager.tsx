'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from './ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import { Textarea } from './ui/textarea';

type TemplateSource = 'custom' | 'bundled' | 'builtIn';

interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  isCustom: boolean;
  source: TemplateSource;
}

interface TemplateJsonResponse extends TemplateInfo {
  templateJson: string;
}

function formatSource(source: TemplateSource) {
  if (source === 'custom') return 'Custom override';
  if (source === 'bundled') return 'Bundled default';
  return 'Built-in default';
}

export function SummaryTemplateManager() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState('standard_meeting');
  const [templateJson, setTemplateJson] = useState('');
  const [currentTemplate, setCurrentTemplate] = useState<TemplateJsonResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isResetting, setIsResetting] = useState(false);
  const [isValidating, setIsValidating] = useState(false);

  const selectedTemplate = useMemo(
    () => templates.find((template) => template.id === selectedTemplateId),
    [selectedTemplateId, templates]
  );

  const loadTemplates = useCallback(async () => {
    const availableTemplates = await invoke<TemplateInfo[]>('api_list_templates');
    setTemplates(availableTemplates);
    if (
      availableTemplates.length > 0 &&
      !availableTemplates.some((template) => template.id === selectedTemplateId)
    ) {
      setSelectedTemplateId(availableTemplates[0].id);
    }
  }, [selectedTemplateId]);

  const loadTemplateJson = useCallback(async (templateId: string) => {
    setIsLoading(true);
    try {
      const template = await invoke<TemplateJsonResponse>('api_get_template_json', {
        templateId,
      });
      setCurrentTemplate(template);
      setTemplateJson(template.templateJson);
    } catch (error) {
      console.error('Failed to load template JSON:', error);
      toast.error('Failed to load template JSON');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTemplates().catch((error) => {
      console.error('Failed to load templates:', error);
      toast.error('Failed to load summary templates');
    });
  }, [loadTemplates]);

  useEffect(() => {
    if (selectedTemplateId) {
      loadTemplateJson(selectedTemplateId);
    }
  }, [loadTemplateJson, selectedTemplateId]);

  const handleValidate = async () => {
    setIsValidating(true);
    try {
      const templateName = await invoke<string>('api_validate_template', {
        templateJson,
      });
      toast.success('Template JSON is valid', {
        description: `Validated "${templateName}"`,
      });
    } catch (error) {
      console.error('Template validation failed:', error);
      toast.error('Template JSON is invalid', {
        description: String(error),
      });
    } finally {
      setIsValidating(false);
    }
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      const savedTemplate = await invoke<TemplateJsonResponse>('api_save_template', {
        templateId: selectedTemplateId,
        templateJson,
      });
      setCurrentTemplate(savedTemplate);
      setTemplateJson(savedTemplate.templateJson);
      await loadTemplates();
      toast.success('Template override saved', {
        description: `Generate Summary will now use "${savedTemplate.name}" for this template id.`,
      });
    } catch (error) {
      console.error('Failed to save template override:', error);
      toast.error('Failed to save template override', {
        description: String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleReset = async () => {
    setIsResetting(true);
    try {
      const resetTemplate = await invoke<TemplateJsonResponse>('api_reset_template', {
        templateId: selectedTemplateId,
      });
      setCurrentTemplate(resetTemplate);
      setTemplateJson(resetTemplate.templateJson);
      await loadTemplates();
      toast.success('Template reset to default', {
        description: `Using ${formatSource(resetTemplate.source)} for "${resetTemplate.name}".`,
      });
    } catch (error) {
      console.error('Failed to reset template:', error);
      toast.error('Failed to reset template', {
        description: String(error),
      });
    } finally {
      setIsResetting(false);
    }
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <h3 className="text-lg font-semibold mb-4">Summary Templates</h3>
      <p className="text-sm text-gray-600 mb-4">
        Customize the JSON templates used by Generate Summary. Saved JSON overrides the bundled default;
        reset removes your override and falls back to the app default.
      </p>

      <div className="space-y-4">
        <div>
          <label className="text-sm font-medium text-gray-900 mb-2 block">
            Template
          </label>
          <Select value={selectedTemplateId} onValueChange={setSelectedTemplateId}>
            <SelectTrigger>
              <SelectValue placeholder="Select a template" />
            </SelectTrigger>
            <SelectContent>
              {templates.map((template) => (
                <SelectItem key={template.id} value={template.id}>
                  {template.name}{template.isCustom ? ' (custom)' : ''}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {selectedTemplate && (
            <p className="text-xs text-gray-600 mt-2">
              {selectedTemplate.description}
            </p>
          )}
          {currentTemplate && (
            <p className="text-xs text-gray-500 mt-1">
              Source: {formatSource(currentTemplate.source)}
            </p>
          )}
        </div>

        <div>
          <div className="flex items-center justify-between mb-2">
            <label className="text-sm font-medium text-gray-900">
              Template JSON
            </label>
            {isLoading && <span className="text-xs text-gray-500">Loading...</span>}
          </div>
          <Textarea
            value={templateJson}
            onChange={(event) => setTemplateJson(event.target.value)}
            className="min-h-[360px] font-mono text-xs leading-relaxed"
            placeholder="Select a template to edit its JSON"
          />
        </div>

        <div className="flex flex-wrap justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={handleValidate}
            disabled={isLoading || isSaving || isResetting || isValidating || !templateJson.trim()}
          >
            {isValidating ? 'Validating...' : 'Validate JSON'}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={handleReset}
            disabled={isLoading || isSaving || isResetting || !currentTemplate?.isCustom}
          >
            {isResetting ? 'Resetting...' : 'Reset to default'}
          </Button>
          <Button
            type="button"
            onClick={handleSave}
            disabled={isLoading || isSaving || isResetting || !templateJson.trim()}
          >
            {isSaving ? 'Saving...' : 'Save template override'}
          </Button>
        </div>
      </div>
    </div>
  );
}
