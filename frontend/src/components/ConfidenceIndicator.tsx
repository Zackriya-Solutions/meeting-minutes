'use client';

interface ConfidenceIndicatorProps {
  confidence: number;
  showIndicator?: boolean;
}

export const ConfidenceIndicator: React.FC<ConfidenceIndicatorProps> = ({
  confidence,
  showIndicator = true,
}) => {
  // Don't render if preference is disabled
  if (!showIndicator) {
    return null;
  }

  // Get color class based on confidence threshold
  const getColorClass = (conf: number): string => {
    if (conf >= 0.8) return 'bg-success'; // 80-100%: High confidence
    if (conf >= 0.7) return 'bg-warning'; // 70-79%: Good confidence
    if (conf >= 0.4) return 'bg-warning'; // 40-79%: Medium confidence
    return 'bg-destructive'; // Below 50%: Low confidence
  };

  // Get descriptive label for accessibility
  const getConfidenceLabel = (conf: number): string => {
    if (conf >= 0.8) return 'High confidence';
    if (conf >= 0.7) return 'Good confidence';
    if (conf >= 0.4) return 'Medium confidence';
    return 'Low confidence';
  };

  const confidencePercent = (confidence * 100).toFixed(0);
  const colorClass = getColorClass(confidence);
  const label = getConfidenceLabel(confidence);

  return (
    <div
      className="flex items-center gap-1"
      title={`${confidencePercent}% confidence - ${label}`}
      aria-label={`Transcription confidence: ${confidencePercent}%`}
    >
      <div
        className={`w-2 h-2 rounded-full ${colorClass} transition-colors duration-200`}
        role="status"
      />
    </div>
  );
};
