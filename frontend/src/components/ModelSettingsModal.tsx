import { useState, useEffect, useRef } from 'react';
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
import { Lock, Unlock, Eye, EyeOff, RefreshCw, CheckCircle2, XCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';

export interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openai' | 'openrouter';
  model: string;
  whisperModel: string;
  apiKey?: string | null;
  ollamaEndpoint?: string | null;
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
  const [ollamaEndpoint, setOllamaEndpoint] = useState<string>(modelConfig.ollamaEndpoint || '');
  const [isLoadingOllama, setIsLoadingOllama] = useState<boolean>(false);
  const [lastFetchedEndpoint, setLastFetchedEndpoint] = useState<string>(modelConfig.ollamaEndpoint || '');
  const [endpointValidationState, setEndpointValidationState] = useState<'valid' | 'invalid' | 'none'>('none');
  const [hasAutoFetched, setHasAutoFetched] = useState<boolean>(false);
  const hasSyncedFromParent = useRef<boolean>(false);
  const hasLoadedInitialConfig = useRef<boolean>(false);

  // URL validation helper
  const validateOllamaEndpoint = (url: string): boolean => {
    if (!url.trim()) return true; // Empty is valid (uses default)
    try {
      const parsed = new URL(url);
      return parsed.protocol === 'http:' || parsed.protocol === 'https:';
    } catch {
      return false;
    }
  };

  // Debounced URL validation with visual feedback
  useEffect(() => {
    const timer = setTimeout(() => {
      const trimmed = ollamaEndpoint.trim();

      if (!trimmed) {
        setEndpointValidationState('none');
      } else if (validateOllamaEndpoint(trimmed)) {
        setEndpointValidationState('valid');
      } else {
        setEndpointValidationState('invalid');
      }
    }, 500); // 500ms debounce

    return () => clearTimeout(timer);
  }, [ollamaEndpoint]);

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

  // Check if Ollama endpoint has changed but models haven't been fetched yet
  const ollamaEndpointChanged = modelConfig.provider === 'ollama' &&
    ollamaEndpoint.trim() !== lastFetchedEndpoint.trim();

  const isDoneDisabled =
    (requiresApiKey && (!apiKey || (typeof apiKey === 'string' && !apiKey.trim()))) ||
    ollamaEndpointChanged;

  useEffect(() => {
    const fetchModelConfig = async () => {
      try {
        const data = (await invoke('api_get_model_config')) as any;
        if (data && data.provider !== null) {
          setModelConfig(data);
          // Sync ollamaEndpoint state with fetched config
          if (data.ollamaEndpoint) {
            setOllamaEndpoint(data.ollamaEndpoint);
            setLastFetchedEndpoint(data.ollamaEndpoint); // Mark as already fetched
          }
          hasLoadedInitialConfig.current = true; // Mark that initial config is loaded
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
        hasLoadedInitialConfig.current = true; // Mark as loaded even on error
      }
    };

    fetchModelConfig();
  }, []);

  // Sync ollamaEndpoint state when modelConfig.ollamaEndpoint changes from parent
  useEffect(() => {
    const endpoint = modelConfig.ollamaEndpoint || '';
    if (endpoint !== ollamaEndpoint) {
      setOllamaEndpoint(endpoint);
      setLastFetchedEndpoint(endpoint); // Mark as synced with parent
    }
    // Only mark as synced if we have a valid provider (prevents race conditions during init)
    if (modelConfig.provider) {
      hasSyncedFromParent.current = true; // Mark that we've received prop value
    }
  }, [modelConfig.ollamaEndpoint, modelConfig.provider]);

  // Reset hasAutoFetched flag and clear models when switching away from Ollama
  useEffect(() => {
    if (modelConfig.provider !== 'ollama') {
      setHasAutoFetched(false); // Reset flag so it can auto-fetch again if user switches back
      setModels([]); // Clear models list
      setError(''); // Clear any error state
    }
  }, [modelConfig.provider]);

  // Manual fetch function for Ollama models
  const fetchOllamaModels = async () => {
    const trimmedEndpoint = ollamaEndpoint.trim();

    // Validate URL if provided
    if (trimmedEndpoint && !validateOllamaEndpoint(trimmedEndpoint)) {
      const errorMsg = 'Invalid Ollama endpoint URL. Must start with http:// or https://';
      setError(errorMsg);
      toast.error(errorMsg);
      return;
    }

    setIsLoadingOllama(true);
    setError(''); // Clear previous errors

    try {
      const endpoint = trimmedEndpoint || null;
      const modelList = (await invoke('get_ollama_models', { endpoint })) as OllamaModel[];
      setModels(modelList);
      setLastFetchedEndpoint(trimmedEndpoint); // Track successful fetch
      toast.success(`Successfully loaded ${modelList.length} Ollama models`);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to load Ollama models';
      setError(errorMsg);
      toast.error(errorMsg);
      console.error('Error loading models:', err);
    } finally {
      setIsLoadingOllama(false);
    }
  };

  // Auto-load models only on initial load, AFTER endpoint sync
  useEffect(() => {
    let mounted = true;

    const initialLoad = async () => {
      // Only fetch if:
      // 1. Initial config has been loaded from backend (prevents race condition)
      // 2. Provider is ollama
      // 3. Endpoint has been synced from parent
      // 4. Haven't auto-fetched yet
      // 5. Component is still mounted
      if (hasLoadedInitialConfig.current &&
          modelConfig.provider === 'ollama' &&
          hasSyncedFromParent.current &&
          !hasAutoFetched &&
          mounted) {
        await fetchOllamaModels();
        setHasAutoFetched(true);
      }
    };

    initialLoad();

    return () => {
      mounted = false;
    };
  }, [ollamaEndpoint, hasAutoFetched, modelConfig.provider]); // Trigger after endpoint or provider changes

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
      ollamaEndpoint: modelConfig.provider === 'ollama' && ollamaEndpoint.trim()
        ? ollamaEndpoint.trim()
        : null,
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

                // Clear error state when switching providers
                setError('');

                // Get safe default model
                const providerModels = modelOptions[provider];
                const defaultModel = providerModels && providerModels.length > 0
                  ? providerModels[0]
                  : ''; // Fallback to empty string instead of undefined

                setModelConfig({
                  ...modelConfig,
                  provider,
                  model: defaultModel,
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
            <Label>Ollama Endpoint (optional)</Label>
            <p className="text-sm text-muted-foreground mt-1 mb-2">
              Leave empty to use http://localhost:11434 or enter a custom endpoint (e.g., http://192.168.1.100:11434)
            </p>
            <div className="flex gap-2 mt-1">
              <div className="relative flex-1">
                <Input
                  type="url"
                  value={ollamaEndpoint}
                  onChange={(e) => {
                    setOllamaEndpoint(e.target.value);
                    // Clear models and errors when endpoint changes to avoid showing stale data
                    if (e.target.value.trim() !== lastFetchedEndpoint.trim()) {
                      setModels([]);
                      setError(''); // Clear error state
                    }
                  }}
                  placeholder="http://localhost:11434"
                  className={cn(
                    "pr-10",
                    endpointValidationState === 'invalid' && "border-red-500"
                  )}
                />
                {endpointValidationState === 'valid' && (
                  <CheckCircle2 className="absolute right-3 top-1/2 -translate-y-1/2 h-5 w-5 text-green-500" />
                )}
                {endpointValidationState === 'invalid' && (
                  <XCircle className="absolute right-3 top-1/2 -translate-y-1/2 h-5 w-5 text-red-500" />
                )}
              </div>
              <Button
                type="button"
                size={'sm'}
                onClick={fetchOllamaModels}
                disabled={isLoadingOllama}
                variant="outline"
                className="whitespace-nowrap"
              >
                {isLoadingOllama ? (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                    Fetching...
                  </>
                ) : (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4" />
                    Fetch Models
                  </>
                )}
              </Button>
            </div>
            {error && (
              <Alert variant="destructive" className="mt-3">
                <AlertDescription className="flex items-center justify-between">
                  <span>{error}</span>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={fetchOllamaModels}
                    className="ml-4 border-red-300 hover:bg-red-50"
                  >
                    Retry
                  </Button>
                </AlertDescription>
              </Alert>
            )}
            {ollamaEndpointChanged && !error && (
              <Alert className="mt-3 border-yellow-500 bg-yellow-50">
                <AlertDescription className="text-yellow-800">
                  Endpoint changed. Please click "Fetch Models" to load models from the new endpoint before saving.
                </AlertDescription>
              </Alert>
            )}
          </div>
        )}

        {modelConfig.provider === 'ollama' && (
          <div>
            <div className="flex items-center justify-between mb-4">
              <h4 className="text-lg font-bold">Available Ollama Models</h4>
              {lastFetchedEndpoint && models.length > 0 && (
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-muted-foreground">Using:</span>
                  <code className="px-2 py-1 bg-muted rounded text-xs">
                    {lastFetchedEndpoint || 'http://localhost:11434'}
                  </code>
                </div>
              )}
            </div>
            {isLoadingOllama ? (
              <div className="text-center py-8 text-muted-foreground">
                <RefreshCw className="mx-auto h-8 w-8 animate-spin mb-2" />
                Loading models...
              </div>
            ) : models.length === 0 ? (
              <Alert className="mb-4">
                <AlertDescription>
                  No models found. Click "Fetch Models" to load available Ollama models.
                </AlertDescription>
              </Alert>
            ) : !ollamaEndpointChanged && (
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
        <Button
          className={cn(
            'px-4 text-sm font-medium text-white rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500',
            isDoneDisabled ? 'bg-gray-400 cursor-not-allowed' : 'bg-blue-600 hover:bg-blue-700'
          )}
          onClick={handleSave}
          disabled={isDoneDisabled}
        >
          Save
        </Button>
      </div>
    </div>
  );
}
