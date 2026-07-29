import React from 'react';
import { CheckCircle, Clock, Loader2, XCircle } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';

export interface ChunkStatus {
  chunk_id: number;
  status: 'pending' | 'processing' | 'completed' | 'failed';
  start_time?: number;
  end_time?: number;
  duration_ms?: number;
  text_preview?: string;
  error_message?: string;
}

export interface ProcessingProgress {
  total_chunks: number;
  completed_chunks: number;
  processing_chunks: number;
  failed_chunks: number;
  estimated_remaining_ms?: number;
  chunks: ChunkStatus[];
}

interface ChunkProgressDisplayProps {
  progress: ProcessingProgress;
  onPause?: () => void;
  onResume?: () => void;
  onCancel?: () => void;
  isPaused?: boolean;
  className?: string;
}

export function ChunkProgressDisplay({
  progress,
  onPause,
  onResume,
  onCancel,
  isPaused = false,
  className = ''
}: ChunkProgressDisplayProps) {
  const t = useT();
  const completionPercentage = progress.total_chunks > 0
    ? Math.round((progress.completed_chunks / progress.total_chunks) * 100)
    : 0;

  const formatDuration = (ms: number) => {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) {
      return `${hours}${t('h')} ${minutes % 60}${t('m')} ${seconds % 60}${t('s')}`;
    } else if (minutes > 0) {
      return `${minutes}${t('m')} ${seconds % 60}${t('s')}`;
    } else {
      return `${seconds}${t('s')}`;
    }
  };

  const formatTimeRemaining = (ms?: number) => {
    if (!ms || ms <= 0) return t('Calculating...');
    return formatDuration(ms);
  };

  const getChunkStatusIcon = (status: ChunkStatus['status']) => {
    switch (status) {
      case 'completed':
        return <CheckCircle size={14} />;
      case 'processing':
        return <Loader2 size={14} className="animate-spin" />;
      case 'failed':
        return <XCircle size={14} />;
      case 'pending':
      default:
        return <Clock size={14} />;
    }
  };

  const getChunkStatusColor = (status: ChunkStatus['status']) => {
    switch (status) {
      case 'completed':
        return 'text-success bg-success/10 border-success/40';
      case 'processing':
        return 'text-primary bg-primary/10 border-primary/40';
      case 'failed':
        return 'text-destructive bg-destructive/10 border-destructive/40';
      case 'pending':
      default:
        return 'text-muted-foreground bg-background border-border';
    }
  };

  return (
    <div className={`bg-background border border-border rounded-lg p-4 ${className}`}>
      {/* Progress Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-3">
          <h3 className="text-lg font-semibold text-foreground">
            {t('Processing Progress')}
          </h3>
          {isPaused && (
            <span className="bg-primary/10 text-primary px-2 py-1 rounded-full text-xs font-medium">
              {t('Paused')}
            </span>
          )}
        </div>

        <div className="flex items-center space-x-2">
          {!isPaused ? (
            <button
              onClick={onPause}
              className="bg-primary hover:bg-primary/90 text-primary-foreground px-3 py-1 rounded text-sm transition-colors"
              disabled={progress.processing_chunks === 0 && progress.completed_chunks === progress.total_chunks}
            >
              {t('Pause')}
            </button>
          ) : (
            <button
              onClick={onResume}
              className="bg-success hover:brightness-110 text-primary-foreground px-3 py-1 rounded text-sm transition-colors"
            >
              {t('Resume')}
            </button>
          )}

          <button
            onClick={onCancel}
            className="bg-destructive hover:opacity-90 text-primary-foreground px-3 py-1 rounded text-sm transition-colors"
          >
            {t('Cancel')}
          </button>
        </div>
      </div>

      {/* Progress Bar */}
      <div className="mb-4">
        <div className="flex items-center justify-between mb-2">
          <span className="text-sm font-medium text-muted-foreground">
            {progress.completed_chunks} {t('of')} {progress.total_chunks} {t('chunks completed')}
          </span>
          <span className="text-sm font-medium text-muted-foreground">
            {completionPercentage}%
          </span>
        </div>

        <div className="w-full bg-muted rounded-full h-2">
          <div
            className="bg-primary h-2 rounded-full transition-all duration-300 ease-out"
            style={{ width: `${completionPercentage}%` }}
          />
        </div>
      </div>

      {/* Processing Stats */}
      <div className="grid grid-cols-4 gap-4 mb-4 text-sm">
        <div className="text-center">
          <div className="text-lg font-semibold text-success">
            {progress.completed_chunks}
          </div>
          <div className="text-muted-foreground">{t('Completed')}</div>
        </div>

        <div className="text-center">
          <div className="text-lg font-semibold text-primary">
            {progress.processing_chunks}
          </div>
          <div className="text-muted-foreground">{t('Processing')}</div>
        </div>

        <div className="text-center">
          <div className="text-lg font-semibold text-muted-foreground">
            {progress.total_chunks - progress.completed_chunks - progress.processing_chunks - progress.failed_chunks}
          </div>
          <div className="text-muted-foreground">{t('Pending')}</div>
        </div>

        <div className="text-center">
          <div className="text-lg font-semibold text-destructive">
            {progress.failed_chunks}
          </div>
          <div className="text-muted-foreground">{t('Failed')}</div>
        </div>
      </div>

      {/* Time Estimate */}
      {progress.estimated_remaining_ms && progress.estimated_remaining_ms > 0 && (
        <div className="bg-primary/10 border border-primary/40 rounded-lg p-3 mb-4">
          <div className="flex items-center space-x-2">
            <Clock size={16} className="text-primary" />
            <span className="text-sm text-primary">
              {t('Estimated time remaining:')} {formatTimeRemaining(progress.estimated_remaining_ms)}
            </span>
          </div>
        </div>
      )}

      {/* Recent Chunks Grid */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium text-muted-foreground mb-2">
          {t('Recent Chunks')} ({Math.min(progress.chunks.length, 10)} {t('of')} {progress.total_chunks})
        </h4>

        <div className="max-h-48 overflow-y-auto space-y-1">
          {progress.chunks
            .slice(-10) // Show last 10 chunks
            .reverse() // Most recent first
            .map((chunk) => (
              <div
                key={chunk.chunk_id}
                className={`text-xs p-2 rounded border ${getChunkStatusColor(chunk.status)}`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <span>{getChunkStatusIcon(chunk.status)}</span>
                    <span className="font-medium">
                      {t('Chunk')} {chunk.chunk_id}
                    </span>
                    {chunk.duration_ms && (
                      <span className="text-muted-foreground">
                        ({formatDuration(chunk.duration_ms)})
                      </span>
                    )}
                  </div>

                  {chunk.status === 'processing' && (
                    <div className="flex items-center space-x-1">
                      <div className="animate-spin w-3 h-3 border border-primary/40 border-t-transparent rounded-full"></div>
                    </div>
                  )}
                </div>

                {chunk.text_preview && (
                  <div className="mt-1 text-muted-foreground text-xs truncate">
                    "{chunk.text_preview}"
                  </div>
                )}

                {chunk.error_message && (
                  <div className="mt-1 text-destructive text-xs">
                    {t('Error:')} {chunk.error_message}
                  </div>
                )}
              </div>
            ))}
        </div>
      </div>

      {/* Processing Complete */}
      {progress.completed_chunks === progress.total_chunks && progress.total_chunks > 0 && (
        <div className="mt-4 bg-success/10 border border-success/40 rounded-lg p-3">
          <div className="flex items-center space-x-2">
            <CheckCircle size={16} className="text-success" />
            <span className="text-sm font-medium text-success">
              {t('Processing completed! All')} {progress.total_chunks} {t('chunks have been transcribed.')}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}

// Mini version for sidebar or compact display
export function ChunkProgressMini({ progress, className = '' }: { progress: ProcessingProgress; className?: string }) {
  const t = useT();
  const completionPercentage = progress.total_chunks > 0
    ? Math.round((progress.completed_chunks / progress.total_chunks) * 100)
    : 0;

  return (
    <div className={`bg-background border border-border rounded-lg p-3 ${className}`}>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-muted-foreground">
          {t('Processing')}
        </span>
        <span className="text-sm font-medium text-muted-foreground">
          {completionPercentage}%
        </span>
      </div>

      <div className="w-full bg-muted rounded-full h-1.5 mb-2">
        <div
          className="bg-primary h-1.5 rounded-full transition-all duration-300"
          style={{ width: `${completionPercentage}%` }}
        />
      </div>

      <div className="text-xs text-muted-foreground">
        {progress.completed_chunks} / {progress.total_chunks} {t('chunks')}
        {progress.processing_chunks > 0 && (
          <span className="ml-2 text-primary">
            ({progress.processing_chunks} {t('processing')})
          </span>
        )}
      </div>
    </div>
  );
}
