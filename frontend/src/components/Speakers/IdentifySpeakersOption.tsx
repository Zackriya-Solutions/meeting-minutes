'use client';

import { useState } from 'react';
import { Switch } from '@/components/ui/switch';
import { useConfig } from '@/contexts/ConfigContext';
import { SpeakerCountSelect } from './SpeakerCountSelect';

/** "Identify speakers" choice for import and retranscription; always off while the beta feature is off. */
export function useSpeakerOptions() {
  const { betaFeatures } = useConfig();
  const [identify, setIdentify] = useState(true);
  const [numSpeakers, setNumSpeakers] = useState<number | null>(null);
  const available = betaFeatures.speakerIdentification;
  return {
    available,
    identify,
    setIdentify,
    numSpeakers,
    setNumSpeakers,
    request: { identify: available && identify, numSpeakers },
  };
}

export type SpeakerOptions = ReturnType<typeof useSpeakerOptions>;

export function IdentifySpeakersOption({ options }: { options: SpeakerOptions }) {
  if (!options.available) return null;
  return (
    <div className="flex items-center justify-between gap-3">
      <div>
        <div className="text-sm font-medium">Identify speakers</div>
        <div className="text-xs text-gray-500">Label who said each line</div>
      </div>
      <div className="flex items-center gap-2">
        {options.identify && <SpeakerCountSelect value={options.numSpeakers} onChange={options.setNumSpeakers} />}
        <Switch checked={options.identify} onCheckedChange={options.setIdentify} />
      </div>
    </div>
  );
}
