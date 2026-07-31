'use client';

import { useT } from '@/lib/i18n';

interface ConfidenceIndicatorProps {
  confidence: number;
  showIndicator?: boolean;
}

export const ConfidenceIndicator: React.FC<ConfidenceIndicatorProps> = ({
  confidence,
  showIndicator = true,
}) => {
  const t = useT();

  // Don't render if preference is disabled
  if (!showIndicator) {
    return null;
  }

  // Get color class based on confidence threshold
  const getColorClass = (conf: number): string => {
    if (conf >= 0.8) return 'bg-success'; // 80-100%: High confidence
    if (conf >= 0.7) return 'bg-primary'; // 70-79%: Good confidence
    if (conf >= 0.4) return 'bg-primary'; // 40-79%: Medium confidence
    return 'bg-destructive'; // Below 50%: Low confidence
  };

  // Get descriptive label for accessibility
  const getConfidenceLabel = (conf: number): string => {
    if (conf >= 0.8) return t('High confidence');
    if (conf >= 0.7) return t('Good confidence');
    if (conf >= 0.4) return t('Medium confidence');
    return t('Low confidence');
  };

  const confidencePercent = (confidence * 100).toFixed(0);
  const colorClass = getColorClass(confidence);
  const label = getConfidenceLabel(confidence);

  return (
    <div
      className="flex items-center gap-1"
      title={`${confidencePercent}% ${t('confidence')} - ${label}`}
      aria-label={`${t('Transcription confidence')}: ${confidencePercent}%`}
    >
      <div
        className={`w-2 h-2 rounded-full ${colorClass} transition-colors duration-200`}
        role="status"
      />
    </div>
  );
};
