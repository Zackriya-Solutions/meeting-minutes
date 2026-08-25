'use client';

import React, { useState, useEffect } from 'react';
import { Button } from './ui/button';
import { X, Video, StopCircle } from 'lucide-react';

interface TeamsDetectionPopupProps {
  variant: 'started' | 'ended';
  countdown?: number;
  onDismiss?: () => void;
  onStart?: () => Promise<void>;
  onStop?: () => void;
  onContinue?: () => void;
  sidebarCollapsed: boolean;
}

export const TeamsDetectionPopup: React.FC<TeamsDetectionPopupProps> = ({
  variant,
  countdown,
  onDismiss,
  onStart,
  onStop,
  onContinue,
  sidebarCollapsed,
}) => {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setVisible(true), 20);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="fixed bottom-4 left-0 right-0 z-50">
      <div
        className="flex justify-center pl-8 transition-[margin] duration-300"
        style={{ marginLeft: sidebarCollapsed ? '4rem' : '16rem' }}
      >
        <div
          className={`transition-all duration-300 ${
            visible ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-2'
          }`}
        >
          {variant === 'started' ? (
            <div className="bg-white border border-green-200 rounded-lg shadow-lg p-3 w-72">
              <div className="flex items-start justify-between mb-1">
                <div className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-green-500 animate-pulse flex-shrink-0 mt-0.5" />
                  <Video className="h-3.5 w-3.5 text-green-600 flex-shrink-0" />
                  <span className="text-sm font-semibold text-gray-900">MS Teams meeting detected</span>
                </div>
                <button
                  onClick={onDismiss}
                  aria-label="Dismiss"
                  className="text-gray-400 hover:text-gray-600 transition-colors p-0.5 rounded hover:bg-gray-100 -mt-0.5 -mr-0.5"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
              <p className="text-xs text-gray-500 mb-3 ml-5">Would you like to record this meeting?</p>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="default"
                  onClick={onStart}
                  className="flex-1 h-7 text-xs bg-green-600 hover:bg-green-700"
                >
                  <Video className="h-3 w-3 mr-1" />
                  Start Recording
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={onDismiss}
                  className="flex-1 h-7 text-xs"
                >
                  Dismiss
                </Button>
              </div>
            </div>
          ) : (
            <div className="bg-white border border-gray-200 rounded-lg shadow-lg p-3 w-72">
              <div className="flex items-start justify-between mb-1">
                <div className="flex items-center gap-1.5">
                  <Video className="h-3.5 w-3.5 text-gray-500 flex-shrink-0" />
                  <span className="text-sm font-semibold text-gray-900">MS Teams meeting ended</span>
                </div>
              </div>
              <p className="text-xs text-gray-500 mb-3">
                Recording will stop in{' '}
                <span className="font-semibold text-gray-700 tabular-nums">{countdown}s</span>
                …
              </p>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={onStop}
                  className="flex-1 h-7 text-xs"
                >
                  <StopCircle className="h-3 w-3 mr-1" />
                  Stop Recording
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={onContinue}
                  className="flex-1 h-7 text-xs"
                >
                  Continue
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
