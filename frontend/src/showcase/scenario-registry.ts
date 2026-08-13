import { createElement, type ComponentType } from 'react';
import DesignTokensShowcase from './foundations/DesignTokens.showcase';
import ButtonShowcase from '@/components/ui/button.showcase';
import AudioLevelMeterShowcase from '@/components/AudioLevelMeter.showcase';
import MeetingDetectionBannerShowcase from '@/components/MeetingDetectionBanner.showcase';
import WordmarkShowcase from '@/components/memento/Wordmark.showcase';
import { ModuleShowcase } from './ModuleShowcase';
import { productionComponentModules } from './AllComponents.showcase';

const generatedScenarios = Object.fromEntries(
  Object.entries(productionComponentModules).map(([id, module]) => [
    id,
    () => createElement(ModuleShowcase, { module, title: id }),
  ]),
) as Record<string, ComponentType>;

export const scenarioRegistry: Record<string, ComponentType> = {
  ...generatedScenarios,
  'design-tokens': DesignTokensShowcase,
  'ui-button': ButtonShowcase,
  'audio-level-meter': AudioLevelMeterShowcase,
  'meeting-detection-banner': MeetingDetectionBannerShowcase,
  'memento-wordmark': WordmarkShowcase,
};
