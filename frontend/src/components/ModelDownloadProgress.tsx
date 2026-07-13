import React from 'react';
import { ModelStatus } from '../lib/whisper';
import { Button } from './ui/button';

interface ModelDownloadProgressProps {
  status: ModelStatus;
  modelName: string;
  onCancel?: () => void;
}

export function ModelDownloadProgress({ status, modelName, onCancel }: ModelDownloadProgressProps) {
  if (typeof status !== 'object' || !('Downloading' in status)) {
    return null;
  }

  const progress = status.Downloading;
  const isCompleted = progress >= 100;

  return (
    <div className="bg-[var(--gold-soft)] border border-[var(--gold-border)] rounded-lg p-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center space-x-2">
          <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-[var(--gold-border)]"></div>
          <span className="text-sm font-medium text-[var(--gold)]">
            {isCompleted ? 'Завершаю…' : `Загружаю ${modelName}`}
          </span>
        </div>
      </div>
      
      <div className="relative">
        <div className="w-full bg-[var(--gold-soft)] rounded-full h-2">
          <div 
            className="bg-[var(--gold)] h-2 rounded-full transition-all duration-300 ease-out"
            style={{ width: `${Math.min(progress, 100)}%` }}
          />
        </div>
        <div className="flex justify-between text-xs text-[var(--gold)] mt-1">
          <span>{Math.round(progress)}% complete</span>
          {!isCompleted && (
            <span className="animate-pulse">Downloading...</span>
          )}
        </div>
      </div>
      
      {isCompleted && (
        <div className="mt-2 text-xs text-[var(--success)]">
          Загрузка завершена, запускаю модель…
        </div>
      )}
    </div>
  );
}

interface ProgressRingProps {
  progress: number;
  size?: number;
  strokeWidth?: number;
}

export function ProgressRing({ progress, size = 40, strokeWidth = 3 }: ProgressRingProps) {
  const radius = (size - strokeWidth) / 2;
  const circumference = radius * 2 * Math.PI;
  const strokeDasharray = circumference;
  const strokeDashoffset = circumference - (progress / 100) * circumference;

  return (
    <div className="relative inline-flex items-center justify-center">
      <svg
        width={size}
        height={size}
        className="transform -rotate-90"
      >
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          stroke="#e5e7eb"
          strokeWidth={strokeWidth}
          fill="transparent"
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          stroke="#3b82f6"
          strokeWidth={strokeWidth}
          strokeDasharray={strokeDasharray}
          strokeDashoffset={strokeDashoffset}
          strokeLinecap="round"
          fill="transparent"
          className="transition-all duration-300 ease-in-out"
        />
      </svg>
      <span className="absolute text-xs font-medium text-[var(--gold)]">
        {Math.round(progress)}%
      </span>
    </div>
  );
}

interface DownloadSummaryProps {
  totalModels: number;
  downloadedModels: number;
  totalSizeMb: number;
}

export function DownloadSummary({ totalModels, downloadedModels, totalSizeMb }: DownloadSummaryProps) {
  const formatSize = (mb: number) => {
    if (mb >= 1000) return `${(mb / 1000).toFixed(1)}GB`;
    return `${mb}MB`;
  };

  return (
    <div className="bg-[var(--bg-sheet)] rounded-lg p-3 text-sm">
      <div className="flex items-center justify-between">
        <span className="text-[var(--fg2)]">
          Доступно моделей: {downloadedModels} из {totalModels}
        </span>
        <span className="text-[var(--fg2)]">
          Всего {formatSize(totalSizeMb)}
        </span>
      </div>
      {downloadedModels > 0 && (
        <div className="mt-1 text-xs text-[var(--success)]">
          Модели работают локально — интернет для расшифровки не нужен
        </div>
      )}
    </div>
  );
}
