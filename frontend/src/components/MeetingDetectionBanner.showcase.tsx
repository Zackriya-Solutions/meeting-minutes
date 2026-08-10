import { MeetingDetectionBanner } from './MeetingDetectionBanner';

export default function MeetingDetectionBannerShowcase() {
  return (
    <MeetingDetectionBanner
      open
      state="suggestion"
      appNames={['Zoom']}
      onPrimaryAction={() => undefined}
      onDismiss={() => undefined}
    />
  );
}
