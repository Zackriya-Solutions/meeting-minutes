"use client"

import { Switch } from "./ui/switch"
import { FlaskConical, AlertCircle } from '@/components/memento/LucideCompat'
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
      <div className="flex items-start gap-3 p-4 bg-[var(--gold-soft)] border border-[var(--gold-border)] rounded-lg">
        <AlertCircle className="h-5 w-5 text-[var(--gold)] flex-shrink-0 mt-0.5" />
        <div className="text-sm text-[var(--gold)]">
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
          className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none"
        >
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-2">
                <FlaskConical className="h-5 w-5 text-[var(--fg2)]" />
                <h3 className="text-lg font-semibold text-[var(--fg1)]">
                  {BETA_FEATURE_NAMES[featureKey]}
                </h3>
                <span className="px-2 py-0.5 text-xs font-medium bg-[var(--gold-soft)] text-[var(--gold)] rounded-full">
                  BETA
                </span>
              </div>
              <p className="text-sm text-[var(--fg2)]">
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
      <div className="p-4 bg-[var(--gold-soft)] border border-[var(--gold-border)] rounded-lg">
        <p className="text-sm text-[var(--gold)]">
          <strong>Note:</strong> When disabled, beta features will be hidden. Your existing meetings remain unaffected.
        </p>
      </div>
    </div>
  );
}
