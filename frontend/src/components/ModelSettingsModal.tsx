import { useState, useEffect } from 'react';
import { useSidebar } from './Sidebar/SidebarProvider';
import { invoke } from '@tauri-apps/api/core';

export interface ModelConfig {
  provider: 'ollama' | 'groq' | 'claude' | 'openai';
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

interface ModelSettingsModalProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSave: (config: ModelConfig) => void;
}

export function ModelSettingsModal({
  modelConfig,
  setModelConfig,
  onSave
}: ModelSettingsModalProps) {
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [error, setError] = useState<string>('');
  const [apiKey, setApiKey] = useState<string | null>(modelConfig.apiKey || null);
  const [showApiKey, setShowApiKey] = useState<boolean>(false);
  const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
  const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
  const { serverAddress } = useSidebar();
  useEffect(() => {
    // if (showModelSettings) {
      const fetchModelConfig = async () => {
        try {
        const data = await invoke('api_get_model_config') as any;
        if (data && data.provider !== null) {

          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
      };

      fetchModelConfig();
    // }
  }, []);

  const fetchApiKey = async (provider: string) => {
    try {
      const data = await invoke('api_get_api_key', {
        provider,
      }) as string;
      setApiKey(data || '');
    } catch (err) {
      console.error('Error fetching API key:', err);
      setApiKey(null);
    }
  };

  const modelOptions = {
    ollama: models.map(model => model.name),
    // claude: ['claude-3-5-sonnet-latest'],
    claude: ['claude-3-5-sonnet-latest','claude-3-5-sonnet-20241022', 'claude-3-5-sonnet-20240620'],
    groq: ['llama-3.3-70b-versatile'],
    openai: [
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
    ]
  };

  const requiresApiKey = modelConfig.provider === 'claude' || modelConfig.provider === 'groq' || modelConfig.provider === 'openai';
  const isDoneDisabled = requiresApiKey && (!apiKey || (typeof apiKey === 'string' && !apiKey.trim()));

  useEffect(() => {
    const loadModels = async () => {
      try {
        const modelList = await invoke('get_ollama_models') as OllamaModel[];
        setModels(modelList);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load Ollama models');
        console.error('Error loading models:', err);
      }
    };

    loadModels();
  }, []);

  const formatSize = (size: number): string => {
    if (size < 1024) {
      return `${size} B`;
    } else if (size < 1024 * 1024) {
      return `${(size / 1024).toFixed(1)} KB`;
    } else if (size < 1024 * 1024 * 1024) {
      return `${(size / (1024 * 1024)).toFixed(1)} MB`;
    } else {
      return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`;
    }
  };

  const handleSave = () => {
    const updatedConfig = { ...modelConfig, apiKey: typeof apiKey === 'string' ? apiKey.trim() || null : null };
    setModelConfig(updatedConfig);
    console.log('ModelSettingsModal - handleSave - Updated ModelConfig:', updatedConfig);
    // setShowModelSettings(false);
    onSave(updatedConfig);
  };

  const handleInputClick = () => {
    if (isApiKeyLocked) {
      setIsLockButtonVibrating(true);
      setTimeout(() => setIsLockButtonVibrating(false), 500);
    }
  };

    // if (!showModelSettings) return null;

  return (
    // <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div>
        <div className="flex justify-between items-center mb-4">
          <h3 className="text-lg font-semibold text-gray-900">Model Settings</h3>
          {/* <button
            // onClick={() => setShowModelSettings(false)}
            className="text-gray-500 hover:text-gray-700"
          >
            <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button> */}
        </div>

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Summarization Model
            </label>
            <div className="flex space-x-2">
              <select
                className="px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                value={modelConfig.provider}
                onChange={(e) => {
                  const provider = e.target.value as ModelConfig['provider'];
                  setModelConfig({
                    ...modelConfig,
                    provider,
                    model: modelOptions[provider][0]
                  });
                  fetchApiKey(provider);
                }}
              >
                <option value="claude">Claude</option>
                <option value="groq">Groq</option>
                <option value="ollama">Ollama</option>
                <option value="openai">OpenAI</option>
              </select>

              <select
                className="flex-1 px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                value={modelConfig.model}
                onChange={(e) => setModelConfig((prev: ModelConfig) => ({ ...prev, model: e.target.value }))}
              >
                {modelOptions[modelConfig.provider].map(model => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {requiresApiKey && (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                API Key
              </label>
              <div className="relative">
                <input
                  type={showApiKey ? "text" : "password"}
                  value={apiKey || ''}
                  onChange={(e) => setApiKey(e.target.value)}
                  disabled={isApiKeyLocked}
                  className={`w-full px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 pr-24 ${
                    isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                  }`}
                  placeholder="Enter your API key"
                />
                {isApiKeyLocked && (
                  <div 
                    onClick={handleInputClick}
                    className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                  />
                    
                  
                )}
                <div className="absolute inset-y-0 right-0 pr-3 flex items-center space-x-2">
                  <button
                    type="button"
                    onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                    className={`text-gray-500 hover:text-gray-700 transition-colors duration-200 ${
                      isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                    }`}
                    title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                  >
                    {isApiKeyLocked ? (
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                      </svg>
                    ) : (
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z" />
                      </svg>
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => setShowApiKey(!showApiKey)}
                    className="text-gray-500 hover:text-gray-700"
                  >
                    {showApiKey ? (
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                      </svg>
                    ) : (
                      <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                      </svg>
                    )}
                  </button>
                </div>
              </div>
            </div>
          )}

          {modelConfig.provider === 'ollama' && (
            <div>
              <h4 className="text-lg font-bold mb-4">Available Ollama Models</h4>
              {error && (
                <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">
                  {error}
                </div>
              )}
              <div className="grid gap-4 max-h-[400px] overflow-y-auto pr-2">
                {models.map((model) => (
                  <div 
                    key={model.id}
                    className={`bg-white p-4 rounded-lg shadow cursor-pointer transition-colors ${
                      modelConfig.model === model.name ? 'ring-2 ring-blue-500 bg-blue-50' : 'hover:bg-gray-50'
                    }`}
                    onClick={() => setModelConfig((prev: ModelConfig) => ({ ...prev, model: model.name }))}
                  >
                    <h3 className="font-bold">{model.name}</h3>
                    <p className="text-gray-600">Size: {model.size}</p>
                    <p className="text-gray-600">Modified: {model.modified}</p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="mt-6 flex justify-end">
          <button
            onClick={handleSave}
            disabled={isDoneDisabled}
            className={`px-4 py-2 text-sm font-medium text-white rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 ${
              isDoneDisabled 
                ? 'bg-gray-400 cursor-not-allowed' 
                : 'bg-blue-600 hover:bg-blue-700'
            }`}
          >
            Done
          </button>
        </div>
      </div>
  );
} 