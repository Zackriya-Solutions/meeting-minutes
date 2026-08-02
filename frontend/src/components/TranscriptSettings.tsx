import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock, Loader2 } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { configService } from '@/services/configService';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai' | 'customStreaming';
    model: string;
    apiKey?: string | null;
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

    // ── Custom streaming (websocket) transcription endpoint state ──────────────
    const [streamingEndpoint, setStreamingEndpoint] = useState<string>('');
    const [streamingModel, setStreamingModel] = useState<string>('');
    const [streamingApiKey, setStreamingApiKey] = useState<string>('');
    const [showStreamingApiKey, setShowStreamingApiKey] = useState<boolean>(false);
    const [isTestingStreaming, setIsTestingStreaming] = useState<boolean>(false);
    const [isSavingStreaming, setIsSavingStreaming] = useState<boolean>(false);

    // Load the saved streaming config when the streaming provider is selected.
    useEffect(() => {
        if (uiProvider !== 'customStreaming') return;
        let cancelled = false;
        configService.getCustomTranscriptionConfig()
            .then((cfg) => {
                if (cancelled || !cfg) return;
                setStreamingEndpoint(cfg.endpoint || '');
                setStreamingModel(cfg.model || '');
                setStreamingApiKey(cfg.apiKey || '');
            })
            .catch((err) => console.error('Failed to load streaming transcription config:', err));
        return () => { cancelled = true; };
    }, [uiProvider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };

    const testStreamingConnection = async () => {
        if (!streamingEndpoint.trim() || !streamingModel.trim()) {
            toast.error('Please enter the endpoint URL and model name first');
            return;
        }
        setIsTestingStreaming(true);
        try {
            const result = await configService.testCustomTranscriptionConnection(
                streamingEndpoint.trim(),
                streamingModel.trim(),
                streamingApiKey.trim() || null,
            );
            toast.success(result.message || 'Connection successful!');
        } catch (err) {
            toast.error(err instanceof Error ? err.message : String(err));
        } finally {
            setIsTestingStreaming(false);
        }
    };

    const saveStreamingConfig = async () => {
        if (!streamingEndpoint.trim() || !streamingModel.trim()) {
            toast.error('Please enter the endpoint URL and model name first');
            return;
        }
        setIsSavingStreaming(true);
        try {
            await configService.saveCustomTranscriptionConfig({
                endpoint: streamingEndpoint.trim(),
                model: streamingModel.trim(),
                apiKey: streamingApiKey.trim() || null,
                protocol: 'voxtral-realtime',
                delayMs: null,
            });
            // Saving also activates the streaming provider on the backend; mirror
            // that into the app config so the rest of the UI stays in sync.
            setTranscriptModelConfig({
                ...transcriptModelConfig,
                provider: 'customStreaming',
                model: streamingModel.trim(),
            });
            toast.success('Realtime transcription endpoint saved');
            if (onModelSelect) onModelSelect();
        } catch (err) {
            toast.error(err instanceof Error ? err.message : String(err));
        } finally {
            setIsSavingStreaming(false);
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
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

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
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
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
                                    if (provider !== 'localWhisper' && provider !== 'parakeet') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">🏠 Local Whisper (High Accuracy)</SelectItem>
                                    <SelectItem value="customStreaming">🌐 Custom Realtime (WebSocket)</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'customStreaming' && (
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

                    {uiProvider === 'customStreaming' && (
                        <div className="mt-2 space-y-4 mx-1">
                            <p className="text-xs text-gray-500">
                                Stream audio to a self-hosted realtime transcription server over
                                WebSocket (e.g. a vLLM instance serving Voxtral Realtime). Transcripts
                                arrive live as you speak.
                            </p>

                            <div>
                                <Label className="block text-sm font-medium text-gray-700 mb-1">
                                    Endpoint URL
                                </Label>
                                <Input
                                    type="text"
                                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                                    value={streamingEndpoint}
                                    onChange={(e) => setStreamingEndpoint(e.target.value)}
                                    placeholder="ws://localhost:8000/v1/realtime"
                                    autoCapitalize="off"
                                    autoCorrect="off"
                                    spellCheck={false}
                                />
                                <p className="text-xs text-gray-400 mt-1">
                                    ws:// or wss:// (http/https is accepted and mapped). The default
                                    path <code>/v1/realtime</code> is added when you give only a host.
                                </p>
                            </div>

                            <div>
                                <Label className="block text-sm font-medium text-gray-700 mb-1">
                                    Model
                                </Label>
                                <Input
                                    type="text"
                                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                                    value={streamingModel}
                                    onChange={(e) => setStreamingModel(e.target.value)}
                                    placeholder="voxtral-mini-transcribe-realtime-2602"
                                    autoCapitalize="off"
                                    autoCorrect="off"
                                    spellCheck={false}
                                />
                            </div>

                            <div>
                                <Label className="block text-sm font-medium text-gray-700 mb-1">
                                    API Key <span className="text-gray-400 font-normal">(optional)</span>
                                </Label>
                                <div className="relative">
                                    <Input
                                        type={showStreamingApiKey ? 'text' : 'password'}
                                        className="pr-12 focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                                        value={streamingApiKey}
                                        onChange={(e) => setStreamingApiKey(e.target.value)}
                                        placeholder="Leave empty if the server needs no auth"
                                        autoCapitalize="off"
                                        autoCorrect="off"
                                        spellCheck={false}
                                    />
                                    <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon"
                                            onClick={() => setShowStreamingApiKey(!showStreamingApiKey)}
                                        >
                                            {showStreamingApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                        </Button>
                                    </div>
                                </div>
                            </div>

                            <div className="flex gap-2 pt-1">
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={testStreamingConnection}
                                    disabled={isTestingStreaming || isSavingStreaming}
                                >
                                    {isTestingStreaming && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
                                    Test Connection
                                </Button>
                                <Button
                                    type="button"
                                    onClick={saveStreamingConfig}
                                    disabled={isSavingStreaming || isTestingStreaming}
                                >
                                    {isSavingStreaming && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
                                    Save
                                </Button>
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
                                    onChange={(e) => setApiKey(e.target.value)}
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
                </div>
            </div>
        </div >
    )
}








