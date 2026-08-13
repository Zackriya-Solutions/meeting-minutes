import { AudioLevelMeter } from './AudioLevelMeter';

export default function AudioLevelMeterShowcase() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <div className="w-full max-w-lg space-y-6 rounded-[var(--ui-radius-20)] bg-[var(--elevation-1)] p-[var(--ui-space-24)]">
        <AudioLevelMeter rmsLevel={0.08} peakLevel={0.14} isActive deviceName="MacBook Microphone" />
        <AudioLevelMeter rmsLevel={0.42} peakLevel={0.61} isActive deviceName="MacBook Microphone" />
        <AudioLevelMeter rmsLevel={0} peakLevel={0} isActive={false} deviceName="MacBook Microphone" />
      </div>
    </main>
  );
}
