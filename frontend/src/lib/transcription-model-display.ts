import type { TranscriptModelProps } from '@/components/TranscriptSettings';

const PROVIDER_LABELS: Record<TranscriptModelProps['provider'], string> = {
  localWhisper: 'Whisper',
  parakeet: 'Parakeet',
  deepgram: 'Deepgram',
  elevenLabs: 'ElevenLabs',
  groq: 'Groq',
  openai: 'OpenAI',
  gemini: 'Gemini',
};

export function formatTranscriptionModelLabel(config: Pick<TranscriptModelProps, 'provider' | 'model'>): string {
  const provider = PROVIDER_LABELS[config.provider] || config.provider;
  const model = config.model?.trim();

  return model ? `${provider} / ${model}` : provider;
}
