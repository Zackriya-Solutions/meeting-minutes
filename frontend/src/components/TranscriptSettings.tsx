import { useState, useEffect } from 'react';
import { useSidebar } from './Sidebar/SidebarProvider';
import { invoke } from '@tauri-apps/api/core';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onSave: (config: TranscriptModelProps) => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onSave }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const { serverAddress } = useSidebar();
    // useEffect(() => {
    //     const fetchTranscriptSettings = async () => {
    //         try {
    //             const response = await fetch(`${serverAddress}/get-transcript-config`);
    //             if (!response.ok) {
    //                 throw new Error(`HTTP error! status: ${response.status}`);
    //             }
    //             const data = await response.json();
    //             if (data.provider !== null) {
    //                 setTranscriptModelConfig(data);
    //                 setApiKey(data.apiKey || null);
    //             }
    //         } catch (error) {
    //             console.error('Failed to fetch transcript settings:', error);
    //         }
    //     };
    //     fetchTranscriptSettings();
    // }, []);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper') {
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
        localWhisper: ['large-v3'],
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq'|| transcriptModelConfig.provider === 'localWhisper';
    const isDoneDisabled = requiresApiKey && (!apiKey || (typeof apiKey === 'string' && !apiKey.trim()));

    const handleSave = () => {
        const updatedConfig = { ...transcriptModelConfig, apiKey: typeof apiKey === 'string' ? apiKey.trim() || null : null };
        setTranscriptModelConfig(updatedConfig);
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
                <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
            </div>
            <div className="space-y-4">
                <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                        Transcript Model
                    </label>
                    <div className="flex space-x-2">
                        <select
                            className="px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                            value={transcriptModelConfig.provider}
                            onChange={(e) => {
                                const provider = e.target.value as TranscriptModelProps['provider'];
                                setTranscriptModelConfig({ ...transcriptModelConfig, provider, model: modelOptions[provider][0] });
                                fetchApiKey(provider);
                            }}
                        >
                            <option value="localWhisper">Local Whisper</option>
                            <option value="deepgram">Deepgram</option>
                            <option value="elevenLabs">ElevenLabs</option>
                            <option value="groq">Groq</option>
                            <option value="openai">OpenAI</option>
                        </select>
                        
                   
                        <select
                            className="px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                            value={transcriptModelConfig.model}
                            onChange={(e) => {
                                const model = e.target.value as TranscriptModelProps['model'];
                                setTranscriptModelConfig({ ...transcriptModelConfig, model });
                            }}
                        >
                            {modelOptions[transcriptModelConfig.provider].map((model) => (
                                <option key={model} value={model}>{model}</option>
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
                            className={`w-full px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 pr-24 ${
                                isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
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
        </div>
    );
}








