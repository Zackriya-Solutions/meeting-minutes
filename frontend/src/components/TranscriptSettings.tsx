import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Label } from './ui/label';
import { GigaamModelManager } from './GigaamModelManager';

export interface TranscriptModelProps {
    // Union kept broad for backward compatibility with stored configs; the UI only
    // offers GigaAM (Russian-market build).
    provider: 'localWhisper' | 'parakeet' | 'gigaam' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

const GIGAAM_MODEL = 'gigaam-v3-e2e-ctc';

/**
 * Transcription settings. GigaAM v3 is the sole transcription engine in this build, so
 * there's no provider picker — just the GigaAM model manager. Any previously-saved
 * provider (Whisper/Parakeet/…) is migrated to GigaAM on mount.
 */
export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig }: TranscriptSettingsProps) {
    useEffect(() => {
        if (transcriptModelConfig.provider !== 'gigaam') {
            const cfg: TranscriptModelProps = { provider: 'gigaam', model: GIGAAM_MODEL, apiKey: null };
            setTranscriptModelConfig(cfg);
            invoke('api_save_transcript_config', { provider: 'gigaam', model: GIGAAM_MODEL, apiKey: null })
                .catch((e) => console.error('Failed to save GigaAM transcript config:', e));
        }
    }, [transcriptModelConfig.provider, setTranscriptModelConfig]);

    return (
        <div className="space-y-4 pb-6">
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">Transcription</Label>
                <p className="text-sm text-gray-500">
                    Meetily transcribes with <span className="font-medium text-gray-700">GigaAM v3</span> (Sber) —
                    on-device Russian speech recognition with punctuation and capitalization.
                </p>
            </div>
            <GigaamModelManager />
        </div>
    );
}
