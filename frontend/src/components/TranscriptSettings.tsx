import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock, X, Plus } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { useConfig } from '@/contexts/ConfigContext';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
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

    // Dictionary and prompt state from context
    const { transcriptionDictionary, setTranscriptionDictionary, transcriptionPrompts, setTranscriptionPrompt } = useConfig();
    const [newTerm, setNewTerm] = useState('');
    const [promptText, setPromptText] = useState(transcriptionPrompts['localWhisper'] || '');
    const promptDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    const PROMPT_MAX_CHARS = 500;
    const supportsPrompt = uiProvider === 'localWhisper';

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    // Keep promptText in sync when provider changes or persisted value changes
    useEffect(() => {
        setPromptText(transcriptionPrompts[uiProvider] || '');
    }, [uiProvider, transcriptionPrompts]);

    const handleAddTerm = useCallback(() => {
        const term = newTerm.trim();
        if (term && !transcriptionDictionary.includes(term)) {
            setTranscriptionDictionary([...transcriptionDictionary, term]);
        }
        setNewTerm('');
    }, [newTerm, transcriptionDictionary, setTranscriptionDictionary]);

    const handleRemoveTerm = useCallback((term: string) => {
        setTranscriptionDictionary(transcriptionDictionary.filter(t => t !== term));
    }, [transcriptionDictionary, setTranscriptionDictionary]);

    const handlePromptChange = useCallback((value: string) => {
        const truncated = value.slice(0, PROMPT_MAX_CHARS);
        setPromptText(truncated);
        if (promptDebounceRef.current) clearTimeout(promptDebounceRef.current);
        promptDebounceRef.current = setTimeout(() => {
            setTranscriptionPrompt(uiProvider, truncated);
        }, 500);
    }, [uiProvider, setTranscriptionPrompt]);

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
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && (
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


                    {/* Dictionary section — shown for all providers (dictionary is universal) */}
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcription Dictionary
                        </Label>
                        <p className="text-xs text-gray-500 mb-2">
                            Words or terms that will be used as vocabulary hints during transcription (e.g. product names, acronyms).
                            {supportsPrompt && ' These are automatically prepended to the initial prompt below.'}
                        </p>
                        <div className="flex gap-2 mx-1 mb-2">
                            <Input
                                value={newTerm}
                                onChange={e => setNewTerm(e.target.value)}
                                onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); handleAddTerm(); } }}
                                placeholder="Add a term..."
                                className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                            />
                            <Button type="button" size="sm" onClick={handleAddTerm} disabled={!newTerm.trim()}>
                                <Plus className="h-4 w-4" />
                            </Button>
                        </div>
                        {transcriptionDictionary.length > 0 && (
                            <div className="mx-1">
                                <div className="flex flex-wrap gap-1 mb-1">
                                    {transcriptionDictionary.map(term => (
                                        <span
                                            key={term}
                                            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 text-xs font-medium border border-blue-200"
                                        >
                                            {term}
                                            <button
                                                type="button"
                                                onClick={() => handleRemoveTerm(term)}
                                                className="hover:text-blue-900 focus:outline-none"
                                                aria-label={`Remove ${term}`}
                                            >
                                                <X className="h-3 w-3" />
                                            </button>
                                        </span>
                                    ))}
                                </div>
                                <button
                                    type="button"
                                    onClick={() => setTranscriptionDictionary([])}
                                    className="text-xs text-gray-400 hover:text-gray-600 underline"
                                >
                                    Clear all
                                </button>
                            </div>
                        )}
                    </div>

                    {/* Initial prompt section — shown only for providers that support it */}
                    {supportsPrompt && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                Initial Prompt
                            </Label>
                            <p className="text-xs text-gray-500 mb-2">
                                An optional hint for the transcription model (e.g. domain context, speaking style, terminology).
                                Dictionary terms above are automatically prepended to this prompt.
                            </p>
                            <div className="mx-1">
                                <Textarea
                                    value={promptText}
                                    onChange={e => handlePromptChange(e.target.value)}
                                    placeholder="Optional hint for the transcription model (e.g. domain context, terminology)..."
                                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500 resize-none"
                                    rows={3}
                                />
                                <div className="text-right text-xs text-gray-400 mt-0.5">
                                    {promptText.length} / {PROMPT_MAX_CHARS}
                                </div>
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








