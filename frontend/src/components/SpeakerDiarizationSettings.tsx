'use client';

import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { SettingsSection } from '@/components/settings/SettingsSection';
import { SettingsRow } from '@/components/settings/SettingsRow';
import { SettingsStatusBadge } from '@/components/settings/SettingsStatusBadge';
import { useConfig } from '@/contexts/ConfigContext';
import type { DiarizationMode } from '@/types/diarization';

export function SpeakerDiarizationSettings() {
  const {
    betaFeatures,
    diarizationSettings,
    isLoadingDiarizationSettings,
    updateDiarizationSettings,
  } = useConfig();

  if (!betaFeatures.speakerDiarization) {
    return null;
  }

  const update = (patch: Partial<typeof diarizationSettings>) => {
    updateDiarizationSettings({ ...diarizationSettings, ...patch })
      .catch(error => console.error('[SpeakerDiarizationSettings] Failed to save:', error));
  };

  const handleEnabledChange = (enabled: boolean) => {
    if (!enabled) {
      update({ enabled: false, mode: 'off' });
      return;
    }

    update({
      enabled: true,
      mode: diarizationSettings.mode === 'off' ? 'live_plus_post_call' : diarizationSettings.mode,
    });
  };

  const handleModeChange = (mode: DiarizationMode) => {
    if (mode === 'off') {
      update({ mode: 'off', enabled: false });
      return;
    }

    update({ mode, enabled: true });
  };

  const liveModeEnabled = diarizationSettings.enabled && diarizationSettings.mode === 'live_plus_post_call';

  return (
    <SettingsSection
      title="Speaker labels"
      description="Label who spoke in the transcript. Live labels are provisional; final labels are applied after the meeting."
      badge={<SettingsStatusBadge tone="beta">Beta</SettingsStatusBadge>}
    >
      <SettingsRow
        title="Enable speaker diarization"
        description="Add speaker labels to transcripts without identifying people by voice."
        control={
          <Switch
            checked={diarizationSettings.enabled}
            disabled={isLoadingDiarizationSettings}
            onCheckedChange={handleEnabledChange}
          />
        }
      />

      <SettingsRow
        title="Mode"
        description="Recommended: show provisional labels during the call, then refine after recording stops."
        control={
          <Select
            value={diarizationSettings.mode}
            disabled={!diarizationSettings.enabled || isLoadingDiarizationSettings}
            onValueChange={(mode) => handleModeChange(mode as DiarizationMode)}
          >
            <SelectTrigger className="w-[260px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="live_plus_post_call">Live + post-call refinement</SelectItem>
              <SelectItem value="post_call_only">Post-call only</SelectItem>
              <SelectItem value="off">Off</SelectItem>
            </SelectContent>
          </Select>
        }
      />

      <SettingsRow
        title="Show provisional labels during call"
        description="Display live Speaker 1 / Speaker 2 labels with a provisional badge."
        disabledReason={!liveModeEnabled ? 'Available only in Live + post-call refinement mode.' : undefined}
        control={
          <Switch
            checked={diarizationSettings.showProvisionalLabels}
            disabled={!liveModeEnabled || isLoadingDiarizationSettings}
            onCheckedChange={(showProvisionalLabels) => update({ showProvisionalLabels })}
          />
        }
      />

      <SettingsRow
        title="Run post-call refinement automatically"
        description="After the meeting, analyze the full audio and replace provisional labels when quality is good."
        control={
          <Switch
            checked={diarizationSettings.postCallRefinementEnabled}
            disabled={!diarizationSettings.enabled || diarizationSettings.mode === 'off' || isLoadingDiarizationSettings}
            onCheckedChange={(postCallRefinementEnabled) => update({ postCallRefinementEnabled })}
          />
        }
      />

      <SettingsRow
        title="Overlap handling"
        description="Interruptions are marked as Multiple speakers."
        control={<SettingsStatusBadge tone="local">Multiple speakers</SettingsStatusBadge>}
      />

      <SettingsRow
        title="Speaker review"
        description="Allow rename, merge, split, and overlap fixes after post-call processing."
        control={
          <Switch
            checked={diarizationSettings.speakerReviewEnabled}
            disabled={!diarizationSettings.enabled || isLoadingDiarizationSettings}
            onCheckedChange={(speakerReviewEnabled) => update({ speakerReviewEnabled })}
          />
        }
      />
    </SettingsSection>
  );
}
