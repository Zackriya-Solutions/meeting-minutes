/**
 * The summary models the app offers, and size formatting shared with the local-model
 * managers in Settings.
 *
 * Summarization is a DeepSeek v4 choice: Pro or Flash, nothing else. The local models below
 * still exist for anyone who opens the advanced providers in Settings, which is why their
 * sizes stay here — but onboarding never offers them.
 */

export const SUMMARY_MODEL_PRO = 'deepseek-v4-pro';
export const SUMMARY_MODEL_FLASH = 'deepseek-v4-flash';

/** Best-quality tier, and the default: a report is read many times, generated once. */
export const DEFAULT_SUMMARY_MODEL = SUMMARY_MODEL_PRO;

export const OFFERED_SUMMARY_MODELS: readonly string[] = [
  SUMMARY_MODEL_PRO,
  SUMMARY_MODEL_FLASH,
];

export function isOfferedSummaryModel(model: string | null | undefined): boolean {
  if (!model) return false;
  return OFFERED_SUMMARY_MODELS.includes(model.trim());
}

/**
 * Coerce a saved or incoming model name to one of the offered tiers. Mirrors
 * `llm::providers::deepseek::normalize_model` on the Rust side: a retired alias
 * (`deepseek-chat`) or a local model left over from an older install becomes the default
 * rather than being written through to the gateway.
 */
export function normalizeSummaryModel(model: string | null | undefined): string {
  if (!model) return DEFAULT_SUMMARY_MODEL;
  const trimmed = model.trim();
  return (
    OFFERED_SUMMARY_MODELS.find((offered) => offered.toLowerCase() === trimmed.toLowerCase()) ??
    DEFAULT_SUMMARY_MODEL
  );
}

const SUMMARY_MODEL_SIZES_MB: Record<string, number> = {
  'qwen3.5:2b': 1221,
  'qwen3.5:4b': 2614,
  'gemma3:1b': 1019,
  'gemma3:4b': 2374,
};

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
