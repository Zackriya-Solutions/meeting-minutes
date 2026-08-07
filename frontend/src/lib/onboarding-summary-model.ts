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
 * tag the Gemma 4 transcription provider uses, so pulling it once serves both —
 * the smaller E2B tier, which is what onboarding downloads.
 * Replaces `gemma3:1b`, which was hardcoded as a literal in four files.
 */
export const RECOMMENDED_SUMMARY_MODEL = 'gemma4:e2b';

// Weights + audio projector, since both files download. Keep in sync with
// `size_mb` + `mmproj.size_mb` in summary_engine/models.rs.
const SUMMARY_MODEL_SIZES_MB: Record<string, number> = {
  'gemma4:e2b': 3651,
  'gemma4:e4b': 5324,
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
