import { useState, useEffect, useRef } from 'react';
import { useSidebar } from './Sidebar/SidebarProvider';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { useOllamaDownload } from '@/contexts/OllamaDownloadContext';
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
import { Switch } from '@/components/ui/switch';
import { Lock, Unlock, Eye, EyeOff, RefreshCw, CheckCircle2, XCircle, ChevronDown, ChevronUp, Download } from 'lucide-react';
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
  skipInitialFetch?: boolean; // Optional: skip fetching config from backend if parent manages it
}

export function ModelSettingsModal({
  modelConfig,
  setModelConfig,
  onSave,
  skipInitialFetch = false,
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
  const [autoGenerateEnabled, setAutoGenerateEnabled] = useState<boolean>(true); // Default to true
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [isEndpointSectionCollapsed, setIsEndpointSectionCollapsed] = useState<boolean>(true); // Collapsed by default

  // Use global download context instead of local state
  const { isDownloading, getProgress, downloadingModels } = useOllamaDownload();

  // Cache models by endpoint to avoid refetching when reverting endpoint changes
  const modelsCache = useRef<Map<string, OllamaModel[]>>(new Map());

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

  // Sync apiKey from parent when it changes
  useEffect(() => {
    if (modelConfig.apiKey !== apiKey) {
      setApiKey(modelConfig.apiKey || null);
    }
  }, [modelConfig.apiKey]);

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
    (modelConfig.provider === 'ollama' && ollamaEndpointChanged);

  useEffect(() => {
    const fetchModelConfig = async () => {
      // If parent component manages config, skip fetch and just mark as loaded
      if (skipInitialFetch) {
        hasLoadedInitialConfig.current = true;
        return;
      }

      try {
        const data = (await invoke('api_get_model_config')) as any;
        if (data && data.provider !== null) {
          setModelConfig(data);

          // Fetch API key if not included in response and provider requires it
          if (data.provider !== 'ollama' && !data.apiKey) {
            try {
              const apiKeyData = await invoke('api_get_api_key', {
                provider: data.provider
              }) as string;
              data.apiKey = apiKeyData;
              setApiKey(apiKeyData);
            } catch (err) {
              console.error('Failed to fetch API key:', err);
            }
          }

          // Sync ollamaEndpoint state with fetched config
          if (data.ollamaEndpoint) {
            setOllamaEndpoint(data.ollamaEndpoint);
            // Don't set lastFetchedEndpoint here - it will be set after successful model fetch
          }
          hasLoadedInitialConfig.current = true; // Mark that initial config is loaded
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
        hasLoadedInitialConfig.current = true; // Mark as loaded even on error
      }
    };

    fetchModelConfig();
  }, [skipInitialFetch]);

  // Fetch auto-generate setting on mount
  useEffect(() => {
    const fetchAutoGenerateSetting = async () => {
      try {
        const enabled = (await invoke('api_get_auto_generate_setting')) as boolean;
        setAutoGenerateEnabled(enabled);
        console.log('Auto-generate setting loaded:', enabled);
      } catch (err) {
        console.error('Failed to fetch auto-generate setting:', err);
        // Keep default value (true) on error
      }
    };

    fetchAutoGenerateSetting();
  }, []);

  // Sync ollamaEndpoint state when modelConfig.ollamaEndpoint changes from parent
  useEffect(() => {
    const endpoint = modelConfig.ollamaEndpoint || '';
    if (endpoint !== ollamaEndpoint) {
      setOllamaEndpoint(endpoint);
      // Don't set lastFetchedEndpoint here - only after successful model fetch
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

  // Handle endpoint changes - restore cached models or clear
  useEffect(() => {
    if (modelConfig.provider === 'ollama' &&
      ollamaEndpoint.trim() !== lastFetchedEndpoint.trim()) {

      // Check if we have cached models for this endpoint (including empty endpoint = default)
      const cachedModels = modelsCache.current.get(ollamaEndpoint.trim());

      if (cachedModels && cachedModels.length > 0) {
        // Restore cached models and update tracking
        setModels(cachedModels);
        setLastFetchedEndpoint(ollamaEndpoint.trim());
        setError('');
      } else {
        // No cache - clear models and allow refetch
        setHasAutoFetched(false);
        setModels([]);
        setError('');
      }
    }
  }, [ollamaEndpoint, lastFetchedEndpoint, modelConfig.provider]);

  // Manual fetch function for Ollama models
  const fetchOllamaModels = async (silent = false) => {
    const trimmedEndpoint = ollamaEndpoint.trim();

    // Validate URL if provided
    if (trimmedEndpoint && !validateOllamaEndpoint(trimmedEndpoint)) {
      const errorMsg = 'Invalid Ollama endpoint URL. Must start with http:// or https://';
      setError(errorMsg);
      if (!silent) {
        toast.error(errorMsg);
      }
      return;
    }

    setIsLoadingOllama(true);
    setError(''); // Clear previous errors

    try {
      const endpoint = trimmedEndpoint || null;
      const modelList = (await invoke('get_ollama_models', { endpoint })) as OllamaModel[];
      setModels(modelList);
      setLastFetchedEndpoint(trimmedEndpoint); // Track successful fetch

      // Cache the fetched models for this endpoint
      modelsCache.current.set(trimmedEndpoint, modelList);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to load Ollama models';
      setError(errorMsg);
      if (!silent) {
        toast.error(errorMsg);
      }
      console.error('Error loading models:', err);
    } finally {
      setIsLoadingOllama(false);
    }
  };

  // Auto-fetch models on initial load only (not on endpoint changes)
  useEffect(() => {
    let mounted = true;

    const initialLoad = async () => {
      // Only auto-fetch on initial load if:
      // 1. Provider is ollama
      // 2. Haven't fetched yet
      // 3. Component is still mounted
      // If skipInitialFetch is true, fetch silently (no error toasts)
      if (modelConfig.provider === 'ollama' &&
        !hasAutoFetched &&
        mounted) {
        await fetchOllamaModels(skipInitialFetch); // Silent if skipInitialFetch=true
        setHasAutoFetched(true);
      }
    };

    initialLoad();

    return () => {
      mounted = false;
    };
  }, [modelConfig.provider]); // Only depend on provider, NOT endpoint

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

  const handleSave = async () => {
    const updatedConfig = {
      ...modelConfig,
      apiKey: typeof apiKey === 'string' ? apiKey.trim() || null : null,
      ollamaEndpoint: modelConfig.provider === 'ollama' && ollamaEndpoint.trim()
        ? ollamaEndpoint.trim()
        : null,
    };
    setModelConfig(updatedConfig);
    console.log('ModelSettingsModal - handleSave - Updated ModelConfig:', updatedConfig);

    // Save auto-generate setting
    // try {
    //   await invoke('api_save_auto_generate_setting', { enabled: autoGenerateEnabled });
    //   console.log('Auto-generate setting saved:', autoGenerateEnabled);
    // } catch (err) {
    //   console.error('Failed to save auto-generate setting:', err);
    //   toast.error('Failed to save auto-generate setting');
    // }

    onSave(updatedConfig);
  };

  const handleInputClick = () => {
    if (isApiKeyLocked) {
      setIsLockButtonVibrating(true);
      setTimeout(() => setIsLockButtonVibrating(false), 500);
    }
  };

  // Function to download recommended model
  const downloadRecommendedModel = async () => {
    const recommendedModel = 'gemma3:1b';

    // Prevent duplicate downloads (defense in depth - backend also checks)
    if (isDownloading(recommendedModel)) {
      toast.info(`${recommendedModel} is already downloading`, {
        description: `Progress: ${Math.round(getProgress(recommendedModel) || 0)}%`
      });
      return;
    }

    try {
      const endpoint = ollamaEndpoint.trim() || null;
      toast.info(`Downloading ${recommendedModel}...`, {
        description: 'This may take a few minutes'
      });

      // The download will be tracked by the global context via events
      await invoke('pull_ollama_model', {
        modelName: recommendedModel,
        endpoint
      });

      // Refresh the models list after successful download
      await fetchOllamaModels(true);

      // Note: Model is NOT auto-selected - user must explicitly choose it
      // This respects the database as the single source of truth
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to download model';
      console.error('Error downloading model:', err);
      // Error toast is handled by the context
    }
  };

  // Function to delete Ollama model
  const deleteOllamaModel = async (modelName: string) => {
    try {
      const endpoint = ollamaEndpoint.trim() || null;
      await invoke('delete_ollama_model', {
        modelName,
        endpoint
      });

      toast.success(`Model ${modelName} deleted`);
      await fetchOllamaModels(true); // Refresh list
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to delete model';
      toast.error(errorMsg);
      console.error('Error deleting model:', err);
    }
  };

  // Track previous downloading models to detect completions
  const previousDownloadingRef = useRef<Set<string>>(new Set());

  // Refresh models list when download completes
  useEffect(() => {
    const current = downloadingModels;
    const previous = previousDownloadingRef.current;

    // Check if any downloads completed (were in previous, not in current)
    for (const modelName of previous) {
      if (!current.has(modelName)) {
        // Download completed, refresh models list
        console.log(`[ModelSettingsModal] Download completed for ${modelName}, refreshing list`);
        fetchOllamaModels(true);
        break; // Only refresh once even if multiple completed
      }
    }

    // Update ref for next comparison
    previousDownloadingRef.current = new Set(current);
  }, [downloadingModels]);

  // Filter Ollama models based on search query
  const filteredModels = models.filter((model) => {
    if (!searchQuery.trim()) return true;

    const query = searchQuery.toLowerCase();
    const isLoaded = modelConfig.model === model.name;
    const loadedText = isLoaded ? 'loaded' : '';

    return (
      model.name.toLowerCase().includes(query) ||
      model.size.toLowerCase().includes(query) ||
      loadedText.includes(query)
    );
  });

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
            <div
              className="flex items-center justify-between cursor-pointer py-2"
              onClick={() => setIsEndpointSectionCollapsed(!isEndpointSectionCollapsed)}
            >
              <Label className="cursor-pointer">Custom Endpoint (optional)</Label>
              {isEndpointSectionCollapsed ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronUp className="h-4 w-4 text-muted-foreground" />
              )}
            </div>

            {!isEndpointSectionCollapsed && (
              <>
                <p className="text-sm text-muted-foreground mt-1 mb-2">
                  Leave empty or enter a custom endpoint (e.g., http://x.yy.zz:11434)
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
                    onClick={() => fetchOllamaModels()}
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
                {ollamaEndpointChanged && !error && (
                  <Alert className="mt-3 border-yellow-500 bg-yellow-50">
                    <AlertDescription className="text-yellow-800">
                      Endpoint changed. Please click "Fetch Models" to load models from the new endpoint before saving.
                    </AlertDescription>
                  </Alert>
                )}
              </>
            )}
          </div>
        )}

        {modelConfig.provider === 'ollama' && (
          <div>
            <div className="flex items-center justify-between mb-4">
              <h4 className="text-sm font-bold">Available Ollama Models</h4>
              {lastFetchedEndpoint && models.length > 0 && (
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-muted-foreground">Using:</span>
                  <code className="px-2 py-1 bg-muted rounded text-xs">
                    {lastFetchedEndpoint || 'http://localhost:11434'}
                  </code>
                </div>
              )}
            </div>
            {models.length > 0 && (
              <div className="mb-4">
                <Input
                  placeholder="Search models..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="w-full"
                />
              </div>
            )}
            {isLoadingOllama ? (
              <div className="text-center py-8 text-muted-foreground">
                <RefreshCw className="mx-auto h-8 w-8 animate-spin mb-2" />
                Loading models...
              </div>
            ) : models.length === 0 ? (
              <div className="space-y-3">
                <Alert className="mb-4">
                  <AlertDescription>
                    {ollamaEndpointChanged
                      ? 'Endpoint changed. Click "Fetch Models" to load models from the new endpoint.'
                      : 'No models found. Download a recommended model or click "Fetch Models" to load available Ollama models.'}
                  </AlertDescription>
                </Alert>
                {!ollamaEndpointChanged && (
                  <div className="space-y-3">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={downloadRecommendedModel}
                      disabled={isDownloading('gemma3:1b')}
                      className="w-full"
                    >
                      {isDownloading('gemma3:1b') ? (
                        <>
                          <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                          Downloading gemma3:1b...
                        </>
                      ) : (
                        <>
                          <Download className="mr-2 h-4 w-4" />
                          Download gemma3:1b (Recommended, ~800MB)
                        </>
                      )}
                    </Button>

                    {/* Show progress for gemma3:1b download */}
                    {isDownloading('gemma3:1b') && getProgress('gemma3:1b') !== undefined && (
                      <div className="bg-white rounded-md border p-3">
                        <div className="flex items-center justify-between mb-2">
                          <span className="text-sm font-medium text-blue-600">Downloading gemma3:1b</span>
                          <span className="text-sm font-semibold text-blue-600">
                            {Math.round(getProgress('gemma3:1b')!)}%
                          </span>
                        </div>
                        <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
                          <div
                            className="h-full bg-gradient-to-r from-blue-500 to-blue-600 rounded-full transition-all duration-300"
                            style={{ width: `${getProgress('gemma3:1b')}%` }}
                          />
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            ) : !ollamaEndpointChanged && (
              <ScrollArea className="max-h-[calc(100vh-450px)] overflow-y-auto pr-4">
                {filteredModels.length === 0 ? (
                  <Alert>
                    <AlertDescription>
                      No models found matching "{searchQuery}". Try a different search term.
                    </AlertDescription>
                  </Alert>
                ) : (
                  <div className="grid gap-4">
                    {filteredModels.map((model) => {
                      const progress = getProgress(model.name);
                      const modelIsDownloading = isDownloading(model.name);

                      return (
                        <div
                          key={model.id}
                          className={cn(
                            'bg-card p-2 m-0 rounded-md border transition-colors',
                            modelConfig.model === model.name
                              ? 'ring-1 ring-blue-500 border-blue-500 background-blue-100'
                              : 'hover:bg-muted/50',
                            !modelIsDownloading && 'cursor-pointer'
                          )}
                          onClick={() => {
                            if (!modelIsDownloading) {
                              setModelConfig((prev: ModelConfig) => ({ ...prev, model: model.name }))
                            }
                          }}
                        >
                          <div>
                            <b className="font-bold">{model.name}&nbsp;</b>
                            <span className="text-muted-foreground">with a size of </span>
                            <span className="font-mono font-bold text-sm">{model.size}</span>
                          </div>

                          {/* Progress bar for downloading models */}
                          {modelIsDownloading && progress !== undefined && (
                            <div className="mt-3 pt-3 border-t border-gray-200">
                              <div className="flex items-center justify-between mb-2">
                                <span className="text-sm font-medium text-blue-600">Downloading...</span>
                                <span className="text-sm font-semibold text-blue-600">{Math.round(progress)}%</span>
                              </div>
                              <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
                                <div
                                  className="h-full bg-gradient-to-r from-blue-500 to-blue-600 rounded-full transition-all duration-300"
                                  style={{ width: `${progress}%` }}
                                />
                              </div>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </ScrollArea>
            )}
          </div>
        )}
      </div>

      {/* Auto-generate summaries toggle */}
      {/* <div className="mt-6 pt-6 border-t border-gray-200">
        <div className="flex items-center justify-between">
          <div className="flex-1">
            <Label htmlFor="auto-generate" className="text-base font-medium">
              Auto-generate summaries
            </Label>
            <p className="text-sm text-muted-foreground mt-1">
              Automatically generate summary when opening meetings without one
            </p>
          </div>
          <Switch
            id="auto-generate"
            checked={autoGenerateEnabled}
            onCheckedChange={setAutoGenerateEnabled}
          />
        </div>
      </div> */}

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
