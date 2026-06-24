import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock, Wifi } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { configService, RemoteConfig } from '@/services/configService';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai' | 'remote';
    model: string;
    apiKey?: string | null;
    /** Remote-only. Backend returns this as a JSON string when present. */
    remoteConfig?: RemoteConfig | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

const REMOTE_BLANK: RemoteConfig = {
    endpointUrl: '',
    bearerToken: '',
    model: '',
    defaultLanguage: '',
    minSpeakers: null,
    maxSpeakers: null,
};

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);

    // Remote-specific form state. Loaded from backend on first mount, kept in sync
    // when the user toggles provider => 'remote'.
    const [remoteDraft, setRemoteDraft] = useState<RemoteConfig>(REMOTE_BLANK);
    const [remoteTestStatus, setRemoteTestStatus] = useState<'idle' | 'testing' | 'ok' | 'fail'>('idle');
    const [remoteTestMessage, setRemoteTestMessage] = useState<string>('');

    // Load remote config on first mount and whenever provider flips to 'remote'.
    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const cfg = await configService.getTranscriptRemoteConfig();
                if (cancelled) return;
                if (cfg) setRemoteDraft(cfg);
            } catch (err) {
                console.error('Failed to load remote config:', err);
            }
        })();
        return () => { cancelled = true; };
    }, []);

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
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const updateRemoteDraft = (patch: Partial<RemoteConfig>) => {
        setRemoteDraft(prev => ({ ...prev, ...patch }));
    };

    const handleTestRemote = async () => {
        setRemoteTestStatus('testing');
        setRemoteTestMessage('');
        try {
            const res = await configService.testTranscriptRemoteConnection(remoteDraft);
            setRemoteTestStatus('ok');
            setRemoteTestMessage(
                `Connected in ${res.elapsedMs} ms; ${res.segmentCount} segment(s)${res.textPreview ? `; preview: "${res.textPreview}"` : ''}`
            );
        } catch (err) {
            setRemoteTestStatus('fail');
            setRemoteTestMessage(String((err as Error)?.message || err));
        }
    };

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
                                    // The four cloud-only providers need a backend API key lookup.
                                    // "remote" carries its own config (endpointUrl + bearerToken
                                    // inside the RemoteConfig JSON) — never look up a stale key.
                                    if (provider !== 'localWhisper' && provider !== 'parakeet' && provider !== 'remote') {
                                        fetchApiKey(provider);
                                    }
                                    // ponytail: clear out cloud-specific api-key state when entering remote.
                                    if (provider === 'remote') {
                                        setApiKey(null);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">🏠 Local Whisper (High Accuracy)</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                    <SelectItem value="remote">🌐 Remote HTTPS (Generic)</SelectItem>
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'remote' && (
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

                    {uiProvider === 'remote' && (
                        <div className="space-y-3 mt-4 border-t pt-4">
                            <div className="grid gap-1">
                                <Label>Endpoint URL</Label>
                                <Input
                                    placeholder="https://your-worker.example.com/transcribe"
                                    value={remoteDraft.endpointUrl}
                                    onChange={(e) => updateRemoteDraft({ endpointUrl: e.target.value })}
                                />
                            </div>
                            <div className="grid gap-1">
                                <Label>Bearer token (optional)</Label>
                                <Input
                                    type="password"
                                    placeholder="sk-…"
                                    value={remoteDraft.bearerToken}
                                    onChange={(e) => updateRemoteDraft({ bearerToken: e.target.value })}
                                />
                            </div>
                            <div className="grid grid-cols-2 gap-3">
                                <div>
                                    <Label>Model id</Label>
                                    <Input
                                        placeholder="e.g. faster-whisper-large-v2"
                                        value={remoteDraft.model}
                                        onChange={(e) => updateRemoteDraft({ model: e.target.value })}
                                    />
                                </div>
                                <div>
                                    <Label>Default language</Label>
                                    <Input
                                        placeholder="ar / en / auto"
                                        value={remoteDraft.defaultLanguage}
                                        onChange={(e) => updateRemoteDraft({ defaultLanguage: e.target.value })}
                                    />
                                </div>
                            </div>
                            <div className="grid grid-cols-2 gap-3">
                                <div>
                                    <Label>Min speakers (optional)</Label>
                                    <Input
                                        type="number"
                                        value={remoteDraft.minSpeakers ?? ''}
                                        onChange={(e) => updateRemoteDraft({ minSpeakers: e.target.value === '' ? null : Number(e.target.value) })}
                                    />
                                </div>
                                <div>
                                    <Label>Max speakers (optional)</Label>
                                    <Input
                                        type="number"
                                        value={remoteDraft.maxSpeakers ?? ''}
                                        onChange={(e) => updateRemoteDraft({ maxSpeakers: e.target.value === '' ? null : Number(e.target.value) })}
                                    />
                                </div>
                            </div>
                            <p className="text-xs text-gray-500">
                                Audio travels to the endpoint above. Default provider remains localWhisper;
                                pick Remote only if you operate or trust a worker.
                            </p>
                            <div className="flex items-center gap-3">
                                <Button
                                    type="button"
                                    onClick={handleTestRemote}
                                    disabled={remoteTestStatus === 'testing' || !remoteDraft.endpointUrl}
                                    className="flex items-center gap-2"
                                >
                                    <Wifi className="w-4 h-4" />
                                    {remoteTestStatus === 'testing' ? 'Testing…' : 'Test connection'}
                                </Button>
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={async () => {
                                        try {
                                            await configService.saveTranscriptRemoteConfig(remoteDraft);
                                            setRemoteTestStatus('idle');
                                            setRemoteTestMessage('Saved');
                                        } catch (err) {
                                            setRemoteTestStatus('fail');
                                            setRemoteTestMessage(String((err as Error)?.message || err));
                                        }
                                    }}
                                >
                                    Save
                                </Button>
                                {remoteTestMessage && (
                                    <span className={
                                        'text-xs ' +
                                        (remoteTestStatus === 'ok' ? 'text-green-700' : remoteTestStatus === 'fail' ? 'text-red-700' : 'text-gray-700')
                                    }>
                                        {remoteTestMessage}
                                    </span>
                                )}
                            </div>
                        </div>
                    )}

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








