"use client"

import { Switch } from "./ui/switch"
import { createMaterialSymbol } from '@/vendor/deslop/primitives/material-symbols-react'
import { useConfig } from "@/contexts/ConfigContext"
import { useT } from "@/lib/i18n"
import {
  BetaFeatureKey,
  BETA_FEATURE_NAMES,
  BETA_FEATURE_DESCRIPTIONS
} from "@/types/betaFeatures"

const IconExperiment = createMaterialSymbol('bolt', 'IconExperiment')

export function BetaSettings() {
  const t = useT();
  const { betaFeatures, toggleBetaFeature } = useConfig();

  // Define feature order for display (allows custom ordering)
  const featureOrder: BetaFeatureKey[] = ['importAndRetranscribe', 'noisyAudioDenoising'];

  return (
    <div className="space-y-6">
      {/* Dynamic Feature Toggles - Automatically renders all features */}
      {featureOrder.map((featureKey) => (
        <div
          key={featureKey}
          className="settings-section settings-cell"
        >
          <div className="settings-cell__row">
            <span className="settings-cell__avatar" aria-hidden="true">
              <IconExperiment size={20} weight={400} />
            </span>
            <div className="settings-cell__text">
              <h3 className="settings-cell__label">
                {t(BETA_FEATURE_NAMES[featureKey])}
              </h3>
              <p className="settings-cell__caption">
                {t(BETA_FEATURE_DESCRIPTIONS[featureKey])}
              </p>
            </div>
            <Switch
              className="shrink-0"
              checked={betaFeatures[featureKey]}
              onCheckedChange={(checked) => toggleBetaFeature(featureKey, checked)}
            />
          </div>
        </div>
      ))}

    </div>
  );
}
