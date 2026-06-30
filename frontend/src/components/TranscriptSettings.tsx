import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock, Wifi } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { configService, RemoteConfig } from '@/services/configService';
import { useConfig } from '@/contexts/ConfigContext';
import { LanguageSelection } from './LanguageSelection';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai' | 'remote' | 'disabled';
    model: string;
    apiKeyVal?: string | null;
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
    const { selectedLanguage, setSelectedLanguage } = useConfig();
    // TranscriptModelProps state is lifted into useConfig(); we render directly from the
    // prop on every render and only persist through the provider/model effect below.
    // No local `uiProvider` indirection: that path re-introduced a sync loop with the
    // mount-time load in app/settings/page.tsx AND ConfigContext's loadTranscriptConfig
    // effect, both of which call setTranscriptModelConfig for the same record.
    const provider = transcriptModelConfig.provider;
    const [apiKeyVal, setApiKeyVal] = useState<string | null>(transcriptModelConfig.apiKeyVal ?? null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);

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

    // Sync apiKey local state when provider flips away from a cloud backend.
    // We do NOT mirror provider changes into local state — provider already lives in
    // `transcriptModelConfig` (controlled). The previous effect `setUiProvider(...)` here
    // formed one half of a Maximum-update-depth-style cycle: settings/page.tsx and
    // ConfigContext each had their own mount-time load effect, both called
    // setTranscriptModelConfig, which propagated to TranscriptSettings and triggered
    // setUiProvider, which re-rendered the controlled <Select>. With apiKey-state
    // synchronization now keyed on provider the only side-effect on provider-change
    // is clearing stale cloud keys, and the effect itself only fires when provider
    // actually differs by React's bailout rules.
    useEffect(() => {
        if (provider === 'localWhisper' || provider === 'parakeet' || provider === 'disabled') {
            setApiKeyVal(null);
        }
    }, [provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKeyVal(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKeyVal(null);
        }
    };

    // Persist provider/model to backend on user-initiated changes only. Two guards:
    //  1. `lastSavedRef` tracks the last triple sent to the backend; we skip the save
    //     when the prop already matches what we persisted. This stops
    //     mount-time loads in settings/page.tsx and ConfigContext from re-triggering
    //     a write.
    //  2. `mountSkipRef` lets the first effect run (the initial mount) pass through
    //     without firing a save — it only snapshots the persisted state.
    //  3. A 500ms debounce coalesces burst flips (e.g. provider/model picked together).
    const lastSavedRef = useRef<{ provider: string; model: string } | null>(null);
    const mountSkipRef = useRef(true);
    useEffect(() => {
        if (mountSkipRef.current) {
            mountSkipRef.current = false;
            lastSavedRef.current = {
                provider: transcriptModelConfig.provider,
                model: transcriptModelConfig.model,
            };
            return;
        }
        const last = lastSavedRef.current;
        if (last && last.provider === transcriptModelConfig.provider && last.model === transcriptModelConfig.model) {
            return;
        }
        const handle = window.setTimeout(() => {
            void configService.saveTranscriptConfig(transcriptModelConfig as TranscriptModelProps)
                .then(() => {
                    lastSavedRef.current = {
                        provider: transcriptModelConfig.provider,
                        model: transcriptModelConfig.model,
                    };
                })
                .catch((err) => console.error('saveTranscriptConfig:', err));
        }, 500);
        return () => window.clearTimeout(handle);
    }, [transcriptModelConfig.provider, transcriptModelConfig.model]);
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['whisper-large-v3', 'whisper-large-v3-turbo', 'distil-whisper-large-v3-en'],
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
                                value={provider}
                                onValueChange={(value) => {
                                    const next = value as TranscriptModelProps['provider'];
                                    // tally: cloud providers need api key backend lookup; remote
                                    // brings its own config in the RemoteConfig JSON.
                                    if (next !== 'localWhisper' && next !== 'parakeet' && next !== 'remote') {
                                        fetchApiKey(next);
                                    }
                                    if (next === 'remote' || next === 'disabled') {
                                        setApiKeyVal(null);
                                    }
                                    // Mirror into lifted state. Persistence is handled by the
                                    // dedicated debounced effect below, never inline — inline
                                    // saves during onValueChange can land mid-render and
                                    // re-trigger the loadTranscriptConfig effects on
                                    // settings/page.tsx and ConfigContext (the sync-loop root).
                                    const nextModel = transcriptModelConfig.model || (
                                        next === 'groq' ? 'whisper-large-v3'
                                            : next === 'localWhisper' ? 'large-v3'
                                                : next === 'parakeet' ? transcriptModelConfig.model
                                                    : ''
                                    );
                                    setTranscriptModelConfig({
                                        ...transcriptModelConfig,
                                        provider: next,
                                        model: nextModel,
                                    } as TranscriptModelProps);
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">🏠 Local Whisper (High Accuracy)</SelectItem>
                                    <SelectItem value="groq">☁️ Groq (Cloud Whisper)</SelectItem>
                                    <SelectItem value="remote">🌐 Remote HTTPS (Generic)</SelectItem>
                                    <SelectItem value="disabled">⏸  Disabled (Recording Only — Low CPU/RAM)</SelectItem>
                                </SelectContent>
                            </Select>

                            {provider !== 'localWhisper' && provider !== 'parakeet' && provider !== 'remote' && provider !== 'disabled' && (
                                (provider === 'groq' || provider === 'deepgram' || provider === 'elevenLabs' || provider === 'openai') && (
                                    <Select
                                        value={transcriptModelConfig.model}
                                        onValueChange={(value) => {
                                            const model = value as TranscriptModelProps['model'];
                                            setTranscriptModelConfig({ ...transcriptModelConfig, provider, model });
                                        }}
                                    >
                                        <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                            <SelectValue placeholder="Select model" />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {modelOptions[provider as keyof typeof modelOptions].map((model) => (
                                                <SelectItem key={model} value={model}>{model}</SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                )
                            )}

                        </div>
                    </div>

                    {provider === 'remote' && (
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

                    {provider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {provider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    <div className="mt-6">
                        <LanguageSelection
                            selectedLanguage={selectedLanguage || 'auto'}
                            onLanguageChange={(lang) => setSelectedLanguage(lang)}
                            provider={provider}
                        />
                    </div>


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
                                    value={apiKeyVal || ''}
                                    onChange={(e) => setApiKeyVal(e.target.value)}
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








