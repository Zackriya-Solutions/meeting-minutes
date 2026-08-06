"use client"

import { Switch } from "./ui/switch"
import { FlaskConical, AlertCircle } from "lucide-react"
import { useConfig } from "@/contexts/ConfigContext"
import {
  BetaFeatureKey,
  BETA_FEATURE_NAMES,
  BETA_FEATURE_DESCRIPTIONS
} from "@/types/betaFeatures"

export function BetaSettings() {
  const { betaFeatures, toggleBetaFeature } = useConfig();

  // Define feature order for display (allows custom ordering)
  const featureOrder: BetaFeatureKey[] = ['importAndRetranscribe'];

  return (
    <div className="space-y-6">
      {/* Yellow Warning Banner */}
      <div className="flex items-start gap-3 p-4 bg-warn-soft border border-warn/40 rounded-lg">
        <AlertCircle className="h-5 w-5 text-warn-ink flex-shrink-0 mt-0.5" />
        <div className="text-sm text-warn-ink">
          <p className="font-medium">Beta Features</p>
          <p className="mt-1">
            These features are still being tested. You may encounter issues, and we appreciate your feedback.
          </p>
        </div>
      </div>

      {/* Dynamic Feature Toggles - Automatically renders all features */}
      {featureOrder.map((featureKey) => (
        <div
          key={featureKey}
          className="bg-elevated rounded-lg border border-line p-6 shadow-sm"
        >
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-2">
                <FlaskConical className="h-5 w-5 text-ink-muted" />
                <h3 className="text-lg font-semibold text-ink">
                  {BETA_FEATURE_NAMES[featureKey]}
                </h3>
                <span className="px-2 py-0.5 text-xs font-medium bg-warn-soft text-warn-ink rounded-full">
                  BETA
                </span>
              </div>
              <p className="text-sm text-ink-muted">
                {BETA_FEATURE_DESCRIPTIONS[featureKey]}
              </p>
            </div>

            <div className="ml-6">
              <Switch
                checked={betaFeatures[featureKey]}
                onCheckedChange={(checked) => toggleBetaFeature(featureKey, checked)}
              />
            </div>
          </div>
        </div>
      ))}

      {/* Info Box */}
      <div className="p-4 bg-info-soft border border-info/40 rounded-lg">
        <p className="text-sm text-info-ink">
          <strong>Note:</strong> When disabled, beta features will be hidden. Your existing meetings remain unaffected.
        </p>
      </div>
    </div>
  );
}
