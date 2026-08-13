'use client';

import React from 'react';
import { Info, Shield } from '@/components/deslop-icons';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { useT } from '@/lib/i18n';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose }: AnalyticsDataModalProps) {
  const t = useT();
  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent size="lg" className="max-w-2xl max-h-[90vh] overflow-y-auto p-0">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-primary" />
            <DialogTitle>{t('What Analytics Collects')}</DialogTitle>
          </div>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-success/10 border border-success/40 rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-success mt-0.5 flex-shrink-0" />
              <div className="text-sm text-success">
                <p className="font-semibold mb-1">{t('Your Privacy is Protected')}</p>
                <p>{t('Analytics is enabled automatically and collects anonymous usage data only. No meeting content, names, file paths, or personal information is ever collected. You can turn analytics off at any time.')}</p>
              </div>
            </div>
          </div>

          <div className="border border-border rounded-lg p-4">
            <h4 className="font-semibold text-foreground mb-2">{t('Where analytics goes')}</h4>
            <p className="text-sm text-muted-foreground">
              {t('When enabled, allowlisted anonymous events are sent to the first-party Memento statistics service at stats.multitool.works. A per-install credential is verified by the Memento gateway; no shared analytics secret or meeting content is sent. Turning analytics off stops the client immediately.')}
            </p>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-foreground">{t('Data We Collect When Enabled:')}</h3>

            {/* Model Preferences */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">{t('1. Model Preferences')}</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• {t('Transcription model (e.g., "Whisper large-v3", "Parakeet")')}</li>
                <li>• {t('Summary model (e.g., "Llama 3.2", "Claude Sonnet")')}</li>
                <li>• {t('Model provider (e.g., "Local", "Ollama", "OpenRouter")')}</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">{t('Helps us understand which models users prefer')}</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">{t('2. Anonymous Meeting Metrics')}</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• {t('Recording duration (e.g., "125 seconds")')}</li>
                <li>• {t('Pause duration (e.g., "5 seconds")')}</li>
                <li>• {t('Number of transcript segments')}</li>
                <li>• {t('Number of audio chunks processed')}</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">{t('Helps us optimize performance and understand usage patterns')}</p>
            </div>

            {/* Device Types */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">{t('3. Device Types (Not Names)')}</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• {t('Microphone type: "Bluetooth" or "Wired" or "Unknown"')}</li>
                <li>• {t('System audio type: "Bluetooth" or "Wired" or "Unknown"')}</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">{t('Helps us improve compatibility, NOT the actual device names')}</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">{t('4. App Usage Patterns')}</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• {t('App started/stopped events')}</li>
                <li>• {t('Session duration')}</li>
                <li>• {t('Feature usage (e.g., "settings changed")')}</li>
                <li>• {t('Error occurrences (helps us fix bugs)')}</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">{t('Helps us improve user experience')}</p>
            </div>

            {/* Platform Info */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">{t('5. Platform Information')}</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• {t('Operating system (e.g., "macOS", "Windows")')}</li>
                <li>• {t('App version (automatically included in all events)')}</li>
                <li>• {t('Architecture (e.g., "x86_64", "aarch64")')}</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">{t('Helps us prioritize platform support')}</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-destructive/10 border border-destructive/40 rounded-lg p-4">
            <h4 className="font-semibold text-destructive mb-2">{t("What We DON'T Collect:")}</h4>
            <ul className="text-sm text-destructive space-y-1 ml-4">
              <li>• {t('Meeting names or titles')}</li>
              <li>• {t('File names, file paths, or meeting folders')}</li>
              <li>• {t('Meeting transcripts or content')}</li>
              <li>• {t('Audio recordings')}</li>
              <li>• {t('Device names (only types: Bluetooth/Wired)')}</li>
              <li>• {t('Personal information')}</li>
              <li>• {t('Any identifiable data')}</li>
            </ul>
          </div>

          {/* Example Event */}
          <div className="bg-background border border-border rounded-lg p-4">
            <h4 className="font-semibold text-foreground mb-2">{t('Example Event:')}</h4>
            <pre className="text-xs text-muted-foreground overflow-x-auto">
              {`{
  "event": "meeting_ended",
  "app_version": "0.4.0",
  "transcription_provider": "parakeet",
  "transcription_model": "parakeet-tdt-0.6b-v3-int8",
  "summary_provider": "ollama",
  "summary_model": "llama3.2:latest",
  "total_duration_seconds": "125.5",
  "microphone_device_type": "Wired",
  "system_audio_device_type": "Bluetooth",
  "chunks_processed": "150",
  "had_fatal_error": "false"
}`}
            </pre>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-4 p-6 border-t border-border bg-background">
          <button
            onClick={onClose}
            className="px-4 py-2 text-muted-foreground bg-background border border-border rounded-md hover:bg-background transition-colors"
          >
            {t('Close')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
