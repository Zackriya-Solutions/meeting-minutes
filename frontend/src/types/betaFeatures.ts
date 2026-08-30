/**
 * Beta Features Type System
 *
 * This file defines the scalable architecture for managing beta features.
 *
 * ## Adding a New Beta Feature
 * 1. Add property to BetaFeatures interface
 * 2. Add default value in DEFAULT_BETA_FEATURES
 * 3. Add analytics mapping in BETA_FEATURE_ANALYTICS_MAP
 * 4. Add UI strings in BETA_FEATURE_NAMES and BETA_FEATURE_DESCRIPTIONS
 * 5. Use in components: `betaFeatures.yourFeatureName`
 *
 * ## Graduating a Feature to Stable
 * 1. Remove property from BetaFeatures interface
 * 2. TypeScript will error at all usage sites
 * 3. Remove conditional checks - feature is now always-on
 */

export interface BetaFeatures {
  /**
   * Import audio files and retranscribe existing meetings with different language settings
   * @since v0.3.0
   */
  importAndRetranscribe: boolean;
}

export const DEFAULT_BETA_FEATURES: BetaFeatures = {
  importAndRetranscribe: true, // Default: enabled
};


/**
 * i18n key mapping for feature names (resolved via useI18n's t() in components)
 */
export const BETA_FEATURE_NAME_KEYS: Record<keyof BetaFeatures, string> = {
  importAndRetranscribe: 'beta.importAndRetranscribeName',
};

/**
 * i18n key mapping for feature descriptions (resolved via useI18n's t() in components)
 */
export const BETA_FEATURE_DESCRIPTION_KEYS: Record<keyof BetaFeatures, string> = {
  importAndRetranscribe: 'beta.importAndRetranscribeDescription',
};

/**
 * Type-safe feature key union
 * This ensures only valid feature keys can be used
 */
export type BetaFeatureKey = keyof BetaFeatures;

/**
 * Load beta features from localStorage
 *
 * @returns BetaFeatures object with values from localStorage or defaults
 */
export function loadBetaFeatures(): BetaFeatures {
  if (typeof window === 'undefined') {
    return { ...DEFAULT_BETA_FEATURES };
  }

  try {
    const saved = localStorage.getItem('betaFeatures');
    if (saved) {
      const parsed = JSON.parse(saved) as Partial<BetaFeatures>;
      // Merge with defaults to handle missing keys (graceful degradation)
      return { ...DEFAULT_BETA_FEATURES, ...parsed };
    }
  } catch (error) {
    console.error('[BetaFeatures] Failed to load from localStorage:', error);
  }

  return { ...DEFAULT_BETA_FEATURES };
}

/**
 * Save beta features to localStorage
 *
 * @param features - BetaFeatures object to save
 */
export function saveBetaFeatures(features: BetaFeatures): void {
  if (typeof window === 'undefined') return;

  try {
    localStorage.setItem('betaFeatures', JSON.stringify(features));
  } catch (error) {
    console.error('[BetaFeatures] Failed to save to localStorage:', error);
  }
}
