import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';

const providerLabels: Partial<Record<TranscriptModelProps['provider'], string>> = {
    parakeet: 'Parakeet (recommended)',
    localWhisper: 'Local Whisper',
    openaiCompatible: 'OpenAI-compatible ASR API',
};

export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'openaiCompatible' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
    endpoint?: string | null;
    whisperModelPath?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = uiProvider === 'openaiCompatible' || transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'localWhisper', // Ensure provider is set correctly
            model: modelName,
            whisperModelPath: null
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const saveConfiguration = async () => {
        const config = {
            ...transcriptModelConfig,
            provider: uiProvider,
            apiKey: apiKey || null,
        };
        await invoke('api_save_transcript_config', {
            provider: config.provider,
            model: config.model,
            apiKey: config.apiKey,
            endpoint: config.endpoint || null,
            whisperModelPath: config.whisperModelPath || null,
        });
        setTranscriptModelConfig(config);
    };

    const handleParakeetModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'parakeet', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    setTranscriptModelConfig({ ...transcriptModelConfig, provider });
                                    if (provider !== 'localWhisper' && provider !== 'parakeet') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider">
                                        {providerLabels[uiProvider] || uiProvider}
                                    </SelectValue>
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">Parakeet (recommended)</SelectItem>
                                    <SelectItem value="localWhisper">Local Whisper</SelectItem>
                                    <SelectItem value="openaiCompatible">OpenAI-compatible ASR API</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'openaiCompatible' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder="Select model" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                            <div className="mt-4 space-y-2">
                                <Label htmlFor="whisper-model-path">Custom GGML model file</Label>
                                <Input
                                    id="whisper-model-path"
                                    value={transcriptModelConfig.whisperModelPath || ''}
                                    onChange={(event) => setTranscriptModelConfig({
                                        ...transcriptModelConfig,
                                        provider: 'localWhisper',
                                        model: event.target.value.split(/[\\/]/).pop()?.replace(/\.bin$/i, '') || 'custom-whisper',
                                        whisperModelPath: event.target.value,
                                    })}
                                    placeholder="/path/to/ggml-model.bin"
                                />
                                <p className="text-xs text-gray-500">
                                    Absolute paths are supported. The file must be a whisper.cpp GGML model; safetensors and PyTorch folders require conversion.
                                </p>
                                <Button type="button" variant="outline" onClick={saveConfiguration}>
                                    Save custom model path
                                </Button>
                            </div>
                        </div>
                    )}

                    {uiProvider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'openaiCompatible' && (
                        <div className="space-y-4">
                            <div>
                                <Label htmlFor="asr-endpoint">ASR endpoint or base URL</Label>
                                <Input
                                    id="asr-endpoint"
                                    value={transcriptModelConfig.endpoint || ''}
                                    onChange={(event) => setTranscriptModelConfig({
                                        ...transcriptModelConfig,
                                        provider: 'openaiCompatible',
                                        endpoint: event.target.value,
                                    })}
                                    placeholder="https://api.openai.com/v1"
                                />
                            </div>
                            <div>
                                <Label htmlFor="asr-model">ASR model</Label>
                                <Input
                                    id="asr-model"
                                    value={transcriptModelConfig.model}
                                    onChange={(event) => setTranscriptModelConfig({
                                        ...transcriptModelConfig,
                                        provider: 'openaiCompatible',
                                        model: event.target.value,
                                    })}
                                    placeholder="gpt-4o-mini-transcribe"
                                />
                            </div>
                        </div>
                    )}


                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => {
                                        setApiKey(e.target.value);
                                        setTranscriptModelConfig({
                                            ...transcriptModelConfig,
                                            provider: uiProvider,
                                            apiKey: e.target.value,
                                        });
                                    }}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder="Enter your API key"
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                    {uiProvider === 'openaiCompatible' && (
                        <Button type="button" onClick={saveConfiguration}>
                            Save ASR service
                        </Button>
                    )}
                </div>
            </div>
        </div >
    )
}





