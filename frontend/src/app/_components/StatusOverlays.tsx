'use client';

import { useTranslations } from 'next-intl';
import { Loader2 } from 'lucide-react';
import { RecordingStatus } from '@/contexts/RecordingStateContext';

interface StatusOverlaysProps {
  status: RecordingStatus;
  statusMessage?: string;
  sidebarCollapsed: boolean;
}

const PROCESSING_STATUSES = [
  RecordingStatus.STOPPING,
  RecordingStatus.PROCESSING_TRANSCRIPTS,
  RecordingStatus.SAVING,
];

export function StatusOverlays({
  status,
  statusMessage,
  sidebarCollapsed,
}: StatusOverlaysProps) {
  const t = useTranslations('summary');
  const currentIndex = PROCESSING_STATUSES.indexOf(status);

  if (currentIndex === -1) return null;

  const labels = [
    t('status.stopping'),
    t('status.finalizing'),
    t('status.saving'),
  ];
  const message = status === RecordingStatus.STOPPING
    ? labels[currentIndex]
    : statusMessage || labels[currentIndex];

  return (
    <div className="fixed bottom-4 left-0 right-0 z-10">
      <div
        className="flex justify-center pl-8 transition-[margin] duration-300"
        style={{ marginLeft: sidebarCollapsed ? '4rem' : '16rem' }}
      >
        <div className="w-2/3 max-w-[750px]">
          <div
            className="bg-white rounded-lg shadow-lg border border-gray-200 px-4 py-3"
            role="status"
            aria-live="polite"
          >
            <div className="flex items-center gap-2 mb-3">
              <Loader2 className="h-4 w-4 animate-spin text-blue-600" />
              <span className="text-sm font-medium text-gray-800">{message}</span>
            </div>
            <div className="flex items-center gap-2" aria-label={message}>
              {labels.map((label, index) => (
                <div key={label} className="flex items-center gap-2 flex-1 last:flex-none">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span
                      className={`h-2.5 w-2.5 rounded-full flex-shrink-0 ${
                        index <= currentIndex ? 'bg-blue-600' : 'bg-gray-300'
                      }`}
                      aria-current={index === currentIndex ? 'step' : undefined}
                    />
                    <span className={`text-xs truncate ${
                      index === currentIndex ? 'text-blue-700 font-medium' : 'text-gray-500'
                    }`}>
                      {label}
                    </span>
                  </div>
                  {index < labels.length - 1 && (
                    <div className={`h-px flex-1 ${index < currentIndex ? 'bg-blue-400' : 'bg-gray-200'}`} />
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
