'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Switch } from './ui/switch';
import { Video } from 'lucide-react';

export function TeamsDetectionSettings() {
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<boolean>('get_teams_detection_enabled')
      .then(v => { setEnabled(v); setLoading(false); })
      .catch(() => setLoading(false));
  }, []);

  const handleChange = async (value: boolean) => {
    setEnabled(value);
    try {
      await invoke('set_teams_detection_enabled', { enabled: value });
    } catch {
      setEnabled(!value);
    }
  };

  if (loading) return null;

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <Video className="w-5 h-5 text-blue-600 mt-0.5 flex-shrink-0" />
          <div>
            <h3 className="text-base font-semibold text-gray-900">
              Auto-detect MS Teams meetings
            </h3>
            <p className="text-sm text-gray-500 mt-1">
              Automatically start recording when a Teams meeting begins and show
              a countdown prompt when it ends.
            </p>
            <p className="text-xs text-gray-400 mt-1">
              macOS: detects via system audio · Windows/Linux: detects via process name
            </p>
          </div>
        </div>
        <Switch checked={enabled} onCheckedChange={handleChange} />
      </div>
    </div>
  );
}
