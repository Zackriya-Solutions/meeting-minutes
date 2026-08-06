interface OnboardingSummaryModelStatusInput {
  selectedModel: string;
  recommendedModel: string;
  selectedModelReady: boolean;
}

interface OnboardingSummaryModelStatus {
  selectedSummaryModel: string;
  summaryModelDownloaded: boolean;
}

/**
 * Ollama model recommended for summarization.
 *
 * Mirrors DEFAULT_SUMMARY_MODEL in src-tauri/src/config.rs. Deliberately the same
 * tag the Gemma 4 transcription provider uses, so pulling it once serves both.
 * Replaces `gemma3:1b`, which was hardcoded as a literal in four files.
 */
export const RECOMMENDED_SUMMARY_MODEL = 'gemma4:e4b';

const SUMMARY_MODEL_SIZES_MB: Record<string, number> = {
  'qwen3.5:2b': 1221,
  'qwen3.5:4b': 2614,
  // Kept so a user who already pulled a Gemma 3 still sees a size for it.
  'gemma3:1b': 1019,
  'gemma3:4b': 2374,
  'gemma4:e2b': 1500,
  'gemma4:e4b': 4400,
};

export function resolveOnboardingSummaryModelStatus({
  selectedModel,
  recommendedModel,
  selectedModelReady,
}: OnboardingSummaryModelStatusInput): OnboardingSummaryModelStatus {
  const selectedSummaryModel = selectedModel || recommendedModel;

  return {
    selectedSummaryModel,
    summaryModelDownloaded: Boolean(selectedSummaryModel && selectedModelReady),
  };
}

export function getSummaryModelSizeMb(model: string): number {
  return SUMMARY_MODEL_SIZES_MB[model] ?? 0;
}

export function getDownloadTotalMb(totalMb: number | null | undefined, model: string): number {
  return totalMb || getSummaryModelSizeMb(model);
}

export function formatSummaryModelSizeLabelFromMb(sizeMb: number): string {
  if (sizeMb === 0) {
    return '';
  }

  if (sizeMb >= 1024) {
    return `~${(sizeMb / 1024).toFixed(1)} GiB`;
  }

  return `~${sizeMb} MiB`;
}

export function getSummaryModelSizeLabel(model: string): string {
  return formatSummaryModelSizeLabelFromMb(getSummaryModelSizeMb(model));
}
