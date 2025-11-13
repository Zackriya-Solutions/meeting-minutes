"use client";

import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Sparkles, Settings, Loader2, FileText, Check } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

interface SummaryGeneratorButtonGroupProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  customPrompt: string;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  availableTemplates: Array<{id: string, name: string, description: string}>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  hasTranscripts?: boolean;
  isModelConfigLoading?: boolean;
}

export function SummaryGeneratorButtonGroup({
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onGenerateSummary,
  customPrompt,
  summaryStatus,
  availableTemplates,
  selectedTemplate,
  onTemplateSelect,
  hasTranscripts = true,
  isModelConfigLoading = false
}: SummaryGeneratorButtonGroupProps) {
  const { t } = useTranslation('common');
  const [isCheckingModels, setIsCheckingModels] = useState(false);
  const [settingsDialogOpen, setSettingsDialogOpen] = useState(false);

  if (!hasTranscripts) {
    return null;
  }

  const checkOllamaModelsAndGenerate = async () => {
    // Only check for Ollama provider
    if (modelConfig.provider !== 'ollama') {
      onGenerateSummary(customPrompt);
      return;
    }

    setIsCheckingModels(true);
    try {
      const endpoint = modelConfig.ollamaEndpoint || null;
      const models = await invoke('get_ollama_models', { endpoint }) as any[];

      if (!models || models.length === 0) {
        // No models available, show message and open settings
        toast.error(
          t('summary.noOllamaModels'),
          { duration: 5000 }
        );
        setSettingsDialogOpen(true);
        return;
      }

      // Models are available, proceed with generation
      onGenerateSummary(customPrompt);
    } catch (error) {
      console.error('Error checking Ollama models:', error);
      toast.error(
        t('summary.ollamaCheckFailed'),
        { duration: 5000 }
      );
      setSettingsDialogOpen(true);
    } finally {
      setIsCheckingModels(false);
    }
  };

  return (
    <ButtonGroup>
      {/* Generate Summary button */}
      <Button
        variant="outline"
        size="sm"
        className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 xl:px-4"
        onClick={() => {
          Analytics.trackButtonClick('generate_summary', 'meeting_details');
          checkOllamaModelsAndGenerate();
        }}
        disabled={summaryStatus === 'processing' || isCheckingModels || isModelConfigLoading}
        title={
          isModelConfigLoading
            ? t('summary.loadingConfig')
            : summaryStatus === 'processing'
            ? t('summary.generating')
            : isCheckingModels
            ? t('summary.checkingModels')
            : t('summary.generate')
        }
      >
        {summaryStatus === 'processing' || isCheckingModels || isModelConfigLoading ? (
          <>
            <Loader2 className="animate-spin xl:mr-2" size={18} />
            <span className="hidden xl:inline">{t('summary.processing')}</span>
          </>
        ) : (
          <>
            <Sparkles className="xl:mr-2" size={18} />
            <span className="hidden lg:inline xl:inline">{t('summary.generateNote')}</span>
          </>
        )}
      </Button>

      {/* Settings button */}
      <Dialog open={settingsDialogOpen} onOpenChange={setSettingsDialogOpen}>
        <DialogTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            title={t('summary.modelSettings')}
          >
            <Settings />
            <span className="hidden lg:inline">{t('summary.aiModel')}</span>
          </Button>
        </DialogTrigger>
        <DialogContent
          aria-describedby={undefined}
        >
          <VisuallyHidden>
            <DialogTitle>{t('summary.modelSettings')}</DialogTitle>
          </VisuallyHidden>
          <ModelSettingsModal
            onSave={async (config) => {
              await onSaveModelConfig(config);
              setSettingsDialogOpen(false);
            }}
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            skipInitialFetch={true}
          />
        </DialogContent>
      </Dialog>

      

      {/* Template selector dropdown */}
      {availableTemplates.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              title={t('summary.selectTemplate')}
            >
              <FileText />
              <span className="hidden lg:inline">{t('summary.template')}</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {availableTemplates.map((template) => (
              <DropdownMenuItem
                key={template.id}
                onClick={() => onTemplateSelect(template.id, template.name)}
                title={template.description}
                className="flex items-center justify-between gap-2"
              >
                <span>{template.name}</span>
                {selectedTemplate === template.id && (
                  <Check className="h-4 w-4 text-green-600" />
                )}
              </DropdownMenuItem>
            ))}

          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </ButtonGroup>
  );
}
