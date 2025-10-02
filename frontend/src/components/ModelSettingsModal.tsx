import { useState, useEffect } from 'react';
import { useSidebar } from './Sidebar/SidebarProvider';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Lock, Unlock, Eye, EyeOff } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openai' | 'openrouter';
  model: string;
  whisperModel: string;
  apiKey?: string | null;
}

interface OllamaModel {
  name: string;
  id: string;
  size: string;
  modified: string;
}

interface OpenRouterModel {
  id: string;
  name: string;
  context_length?: number;
  prompt_price?: string;
  completion_price?: string;
}

interface ModelSettingsModalProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSave: (config: ModelConfig) => void;
}

export function ModelSettingsModal({
  modelConfig,
  setModelConfig,
  onSave,
}: ModelSettingsModalProps) {
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [error, setError] = useState<string>('');
  const [apiKey, setApiKey] = useState<string | null>(modelConfig.apiKey || null);
  const [showApiKey, setShowApiKey] = useState<boolean>(false);
  const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
  const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
  const { serverAddress } = useSidebar();
  const [openRouterModels, setOpenRouterModels] = useState<OpenRouterModel[]>([]);
  const [openRouterError, setOpenRouterError] = useState<string>('');
  const [isLoadingOpenRouter, setIsLoadingOpenRouter] = useState<boolean>(false);

  const fetchApiKey = async (provider: string) => {
    try {
      const data = (await invoke('api_get_api_key', {
        provider,
      })) as string;
      setApiKey(data || '');
    } catch (err) {
      console.error('Error fetching API key:', err);
      setApiKey(null);
    }
  };

  const modelOptions = {
    ollama: models.map((model) => model.name),
    claude: ['claude-3-5-sonnet-latest', 'claude-3-5-sonnet-20241022', 'claude-3-5-sonnet-20240620'],
    groq: ['llama-3.3-70b-versatile'],
    openai: [
      'gpt-5',
      'gpt-5-mini',
      'gpt-4o',
      'gpt-4.1',
      'gpt-4-turbo',
      'gpt-3.5-turbo',
      'gpt-4o-2024-11-20',
      'gpt-4o-2024-08-06',
      'gpt-4o-mini-2024-07-18',
      'gpt-4.1-2025-04-14',
      'gpt-4.1-nano-2025-04-14',
      'gpt-4.1-mini-2025-04-14',
      'o4-mini-2025-04-16',
      'o3-2025-04-16',
      'o3-mini-2025-01-31',
      'o1-2024-12-17',
      'o1-mini-2024-09-12',
      'gpt-4-turbo-2024-04-09',
      'gpt-4-0125-Preview',
      'gpt-4-vision-preview',
      'gpt-4-1106-Preview',
      'gpt-3.5-turbo-0125',
      'gpt-3.5-turbo-1106'
    ],
    openrouter: openRouterModels.map((m) => m.id),
  };

  const requiresApiKey =
    modelConfig.provider === 'claude' ||
    modelConfig.provider === 'groq' ||
    modelConfig.provider === 'openai' ||
    modelConfig.provider === 'openrouter';
  const isDoneDisabled =
    requiresApiKey && (!apiKey || (typeof apiKey === 'string' && !apiKey.trim()));

  useEffect(() => {
    const fetchModelConfig = async () => {
      try {
        const data = (await invoke('api_get_model_config')) as any;
        if (data && data.provider !== null) {
          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
    };

    fetchModelConfig();
  }, []);

  useEffect(() => {
    const loadModels = async () => {
      try {
        const modelList = (await invoke('get_ollama_models')) as OllamaModel[];
        setModels(modelList);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load Ollama models');
        console.error('Error loading models:', err);
      }
    };

    loadModels();
  }, []);

  const loadOpenRouterModels = async () => {
    if (openRouterModels.length > 0) return; // Already loaded
    
    try {
      setIsLoadingOpenRouter(true);
      setOpenRouterError('');
      const data = (await invoke('get_openrouter_models')) as OpenRouterModel[];
      setOpenRouterModels(data);
    } catch (err) {
      console.error('Error loading OpenRouter models:', err);
      setOpenRouterError(
        err instanceof Error ? err.message : 'Failed to load OpenRouter models'
      );
    } finally {
      setIsLoadingOpenRouter(false);
    }
  };

  const handleSave = () => {
    const updatedConfig = {
      ...modelConfig,
      apiKey: typeof apiKey === 'string' ? apiKey.trim() || null : null,
    };
    setModelConfig(updatedConfig);
    console.log('ModelSettingsModal - handleSave - Updated ModelConfig:', updatedConfig);
    onSave(updatedConfig);
  };

  const handleInputClick = () => {
    if (isApiKeyLocked) {
      setIsLockButtonVibrating(true);
      setTimeout(() => setIsLockButtonVibrating(false), 500);
    }
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-4">
        <h3 className="text-lg font-semibold">Model Settings</h3>
      </div>

      <div className="space-y-4">
        <div>
          <Label>Summarization Model</Label>
          <div className="flex space-x-2 mt-1">
            <Select
              value={modelConfig.provider}
              onValueChange={(value) => {
                const provider = value as ModelConfig['provider'];
                setModelConfig({
                  ...modelConfig,
                  provider,
                  model: modelOptions[provider][0],
                });
                fetchApiKey(provider);
                
                // Load OpenRouter models only when OpenRouter is selected
                if (provider === 'openrouter') {
                  loadOpenRouterModels();
                }
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select provider" />
              </SelectTrigger>
              <SelectContent className="max-h-64 overflow-y-auto">
                <SelectItem value="claude">Claude</SelectItem>
                <SelectItem value="groq">Groq</SelectItem>
                <SelectItem value="ollama">Ollama</SelectItem>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="openrouter">OpenRouter</SelectItem>
              </SelectContent>
            </Select>

            <Select
              value={modelConfig.model}
              onValueChange={(value) =>
                setModelConfig((prev: ModelConfig) => ({ ...prev, model: value }))
              }
            >
              <SelectTrigger className="flex-1">
                <SelectValue placeholder="Select model" />
              </SelectTrigger>
              <SelectContent className="max-h-48 overflow-y-auto">
                {modelConfig.provider === 'openrouter' && isLoadingOpenRouter ? (
                  <SelectItem value="loading" disabled>
                    Loading models...
                  </SelectItem>
                ) : (
                  modelOptions[modelConfig.provider].map((model) => (
                    <SelectItem key={model} value={model}>
                      {model}
                    </SelectItem>
                  ))
                )}
              </SelectContent>
            </Select>
          </div>
        </div>

        {requiresApiKey && (
          <div>
            <Label>API Key</Label>
            <div className="relative mt-1">
              <Input
                type={showApiKey ? 'text' : 'password'}
                value={apiKey || ''}
                onChange={(e) => setApiKey(e.target.value)}
                disabled={isApiKeyLocked}
                placeholder="Enter your API key"
                className="pr-24"
              />
              {isApiKeyLocked && (
                <div
                  onClick={handleInputClick}
                  className="absolute inset-0 flex items-center justify-center bg-muted/50 rounded-md cursor-not-allowed"
                />
              )}
              <div className="absolute inset-y-0 right-0 pr-1 flex items-center space-x-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                  className={isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''}
                  title={isApiKeyLocked ? 'Unlock to edit' : 'Lock to prevent editing'}
                >
                  {isApiKeyLocked ? <Lock /> : <Unlock />}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  onClick={() => setShowApiKey(!showApiKey)}
                >
                  {showApiKey ? <EyeOff /> : <Eye />}
                </Button>
              </div>
            </div>
          </div>
        )}

        {modelConfig.provider === 'ollama' && (
          <div>
            <h4 className="text-lg font-bold mb-4">Available Ollama Models</h4>
            {error ? (
              <Alert variant="destructive" className="mb-4">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : (
              <ScrollArea className="max-h-[calc(100vh-450px)] overflow-y-auto pr-4">
                <div className="grid gap-4">
                  {models.map((model) => (
                    <div
                      key={model.id}
                      className={cn(
                        'bg-card p-4 m-2 rounded-lg border cursor-pointer transition-colors',
                        modelConfig.model === model.name
                          ? 'ring-1 ring-blue-500'
                          : 'hover:bg-muted/50'
                      )}
                      onClick={() =>
                        setModelConfig((prev: ModelConfig) => ({ ...prev, model: model.name }))
                      }
                    >
                      <h3 className="font-bold">{model.name}</h3>
                      <p className="text-muted-foreground">Size: {model.size}</p>
                      <p className="text-muted-foreground">Modified: {model.modified}</p>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            )}
          </div>
        )}
      </div>

      <div className="mt-6 flex justify-end">
        <Button className={`px-4 text-sm font-medium text-white rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 ${isDoneDisabled
          ? 'bg-gray-400 cursor-not-allowed'
          : 'bg-blue-600 hover:bg-blue-700'
          }`} onClick={handleSave} disabled={isDoneDisabled}>
          Save
        </Button>
      </div>
    </div>
  );
}