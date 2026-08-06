import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import TranscriptionModelManager from './TranscriptionModelManager';


export interface TranscriptModelProps {
    provider: 'local' | 'builtin-ai' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
}

interface BuiltinTranscribeModel {
    name: string;
    size_mb: number;
    description: string;
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
        if (transcriptModelConfig.provider === 'local') {
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
        local: [], // Model selection handled by TranscriptionModelManager
        'builtin-ai': [], // Model selection handled by the Gemma 4 list below
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

    const handleModelSelect = (modelName: string, provider: TranscriptModelProps['provider'] = 'local') => {
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider,
            model: modelName
        });
        if (onModelSelect) {
            onModelSelect();
        }
    };

    // Built-in audio transcription (Gemma 4), run by the bundled sidecar. The
    // audio-capable models come from the Rust catalog so they have one definition,
    // and download state reuses the existing built-in AI flow.
    const [audioModels, setAudioModels] = useState<BuiltinTranscribeModel[]>([]);
    const [downloadedModels, setDownloadedModels] = useState<string[]>([]);
    const [downloading, setDownloading] = useState<string | null>(null);
    const [audioError, setAudioError] = useState<string | null>(null);

    const refreshAudioModels = async () => {
        try {
            const [catalog, installed] = await Promise.all([
                invoke<BuiltinTranscribeModel[]>('transcribe_builtin_models'),
                invoke<{ name: string; status: { type: string } }[]>('builtin_ai_list_models'),
            ]);
            setAudioModels(catalog);
            setDownloadedModels(
                installed.filter((m) => m.status.type === 'available').map((m) => m.name),
            );
            setAudioError(null);
        } catch (err) {
            setAudioError(String(err));
        }
    };

    useEffect(() => {
        if (uiProvider === 'builtin-ai') {
            refreshAudioModels();
        }
    }, [uiProvider]);

    const downloadAudioModel = async (modelName: string) => {
        setDownloading(modelName);
        setAudioError(null);
        try {
            await invoke('builtin_ai_download_model', { modelName });
            await refreshAudioModels();
        } catch (err) {
            setAudioError(String(err));
        } finally {
            setDownloading(null);
        }
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-ink">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-ink mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    if (provider !== 'local') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-ring focus:border-info/40'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="local">🏠 On-device (Recommended - Real-time)</SelectItem>
                                    <SelectItem value="builtin-ai">✨ Built-in AI — Gemma 4 (audio LLM)</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'local' && uiProvider !== 'builtin-ai' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-ring focus:border-info/40'>
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

                    {uiProvider === 'local' && (
                        <div className="mt-6">
                            <TranscriptionModelManager
                                selectedModel={transcriptModelConfig.provider === 'local' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'builtin-ai' && (
                        <div className="mt-6 space-y-3">
                            <p className="mx-1 text-xs text-ink-muted">
                                Gemma 4 transcribes one sentence at a time rather than word by
                                word, so text appears after each pause. It runs inside the app —
                                nothing else to install — and the same model writes your summaries.
                            </p>

                            {audioError && (
                                <div className="rounded-md bg-danger-soft p-3 text-sm text-danger-ink">
                                    {audioError}
                                </div>
                            )}

                            {audioModels.map((model) => {
                                const installed = downloadedModels.includes(model.name);
                                const selected =
                                    transcriptModelConfig.provider === 'builtin-ai' &&
                                    transcriptModelConfig.model === model.name;

                                return (
                                    <div
                                        key={model.name}
                                        className={`rounded-lg border p-4 ${selected ? 'border-info/40 bg-info-soft' : 'border-line'}`}
                                    >
                                        <div className="flex items-start justify-between gap-4">
                                            <div className="min-w-0">
                                                <span className="font-medium">{model.name}</span>
                                                <p className="mt-1 text-sm text-ink-muted">{model.description}</p>
                                                <p className="mt-1 text-xs text-ink-muted">
                                                    {(model.size_mb / 1024).toFixed(1)} GB
                                                </p>
                                            </div>
                                            <div className="flex shrink-0 items-center gap-2">
                                                {installed && !selected && (
                                                    <Button size="sm" onClick={() => handleModelSelect(model.name, 'builtin-ai')}>
                                                        Use
                                                    </Button>
                                                )}
                                                {selected && (
                                                    <span className="text-sm font-medium text-info-ink">Selected</span>
                                                )}
                                                {!installed && (
                                                    <Button
                                                        size="sm"
                                                        variant="outline"
                                                        disabled={downloading !== null}
                                                        onClick={() => downloadAudioModel(model.name)}
                                                    >
                                                        {downloading === model.name ? 'Downloading…' : 'Download'}
                                                    </Button>
                                                )}
                                            </div>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    )}


                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-ink mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-ring focus:border-info/40 ${isApiKeyLocked ? 'bg-sunken cursor-not-allowed' : ''
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
                                        className="absolute inset-0 flex items-center justify-center bg-sunken bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-danger-ink' : ''
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








