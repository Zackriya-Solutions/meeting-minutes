import React from 'react';

interface AudioLevelMeterProps {
  rmsLevel: number;    // 0.0 to 1.0
  peakLevel: number;   // 0.0 to 1.0
  isActive: boolean;   // Whether audio is being detected
  deviceName: string;
  className?: string;
  size?: 'small' | 'medium' | 'large';
}

export function AudioLevelMeter({
  rmsLevel,
  peakLevel,
  isActive,
  deviceName,
  className = '',
  size = 'medium'
}: AudioLevelMeterProps) {
  // Normalize levels to 0-1 range and apply log scaling for better visual representation
  const normalizedRms = Math.max(0, Math.min(1, rmsLevel));
  const normalizedPeak = Math.max(0, Math.min(1, peakLevel));

  // Apply logarithmic scaling for better visual representation of audio levels
  const logRms = normalizedRms > 0 ? Math.log10(normalizedRms * 9 + 1) : 0;
  const logPeak = normalizedPeak > 0 ? Math.log10(normalizedPeak * 9 + 1) : 0;

  // Calculate percentages for display
  const rmsPercent = Math.round(logRms * 100);
  const peakPercent = Math.round(logPeak * 100);

  // Color coding based on level
  const getLevelColor = (level: number) => {
    if (level < 0.3) return 'bg-green-500';
    if (level < 0.7) return 'bg-yellow-500';
    return 'bg-red-500';
  };

  const rmsColor = getLevelColor(logRms);
  const peakColor = getLevelColor(logPeak);

  // Size variants
  const sizeClasses = {
    small: {
      container: 'h-2',
      text: 'text-xs',
      meter: 'h-1.5'
    },
    medium: {
      container: 'h-3',
      text: 'text-sm',
      meter: 'h-2'
    },
    large: {
      container: 'h-4',
      text: 'text-base',
      meter: 'h-3'
    }
  };

  const sizes = sizeClasses[size];

  return (
    <div className={`flex items-center space-x-2 ${className}`}>
      {/* Device activity indicator */}
      <div className={`w-2 h-2 rounded-full ${
        isActive ? 'bg-green-400 animate-pulse' : 'bg-gray-300'
      }`} title={`${deviceName} - ${isActive ? 'Active' : 'Inactive'}`} />

      {/* Level meter container */}
      <div className={`flex-1 ${sizes.container} relative`}>
        {/* Background */}
        <div className="w-full h-full bg-gray-200 rounded-sm overflow-hidden">
          {/* RMS level bar (main level) */}
          <div
            className={`${sizes.meter} ${rmsColor} transition-all duration-150 ease-out rounded-sm`}
            style={{ width: `${rmsPercent}%` }}
          />

          {/* Peak level indicator (thin line) */}
          {peakPercent > rmsPercent && (
            <div
              className={`absolute top-0 bottom-0 w-0.5 ${peakColor} transition-all duration-75`}
              style={{ left: `${peakPercent}%` }}
            />
          )}
        </div>

        {/* Level markers */}
        <div className="absolute inset-0 flex justify-between items-center px-1 pointer-events-none">
          {/* 25% marker */}
          <div className="w-px h-full bg-gray-400 opacity-30" style={{ marginLeft: '25%' }} />
          {/* 50% marker */}
          <div className="w-px h-full bg-gray-400 opacity-30" style={{ marginLeft: '50%' }} />
          {/* 75% marker */}
          <div className="w-px h-full bg-gray-400 opacity-30" style={{ marginLeft: '75%' }} />
        </div>
      </div>

      {/* Level percentage display */}
      <div className={`${sizes.text} text-gray-600 font-mono min-w-[3rem] text-right`}>
        {rmsPercent}%
      </div>
    </div>
  );
}

interface CompactAudioLevelMeterProps {
  rmsLevel: number;
  peakLevel: number;
  isActive: boolean;
  className?: string;
}

// Compact version for inline display in dropdowns
export function CompactAudioLevelMeter({
  rmsLevel,
  peakLevel,
  isActive,
  className = ''
}: CompactAudioLevelMeterProps) {
  const normalizedRms = Math.max(0, Math.min(1, rmsLevel));
  const logRms = normalizedRms > 0 ? Math.log10(normalizedRms * 9 + 1) : 0;
  const rmsPercent = Math.round(logRms * 100);

  const getLevelColor = (level: number) => {
    if (level < 0.3) return 'bg-green-400';
    if (level < 0.7) return 'bg-yellow-400';
    return 'bg-red-400';
  };

  return (
    <div className={`flex items-center space-x-1 ${className}`}>
      {/* Activity dot */}
      <div className={`w-1.5 h-1.5 rounded-full ${
        isActive ? 'bg-green-400' : 'bg-gray-300'
      }`} />

      {/* Mini meter */}
      <div className="w-8 h-1.5 bg-gray-200 rounded-sm overflow-hidden">
        <div
          className={`h-full ${getLevelColor(logRms)} transition-all duration-150`}
          style={{ width: `${rmsPercent}%` }}
        />
      </div>
    </div>
  );
}