'use client';

import React from 'react';
import { X, Info, Shield } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose }: AnalyticsDataModalProps) {
  const t = useT();
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-[var(--bg-canvas)] rounded-lg shadow-none max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--border-subtle)]">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-[var(--gold)]" />
            <h2 className="text-xl font-semibold text-[var(--fg1)]">{t('What Analytics Collects')}</h2>
          </div>
          <button
            onClick={onClose}
            className="text-[var(--fg3)] hover:text-[var(--fg2)] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-[color-mix(in_srgb,var(--success)_12%,transparent)] border border-[color-mix(in_srgb,var(--success)_42%,transparent)] rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-[var(--success)] mt-0.5 flex-shrink-0" />
              <div className="text-sm text-[var(--success)]">
                <p className="font-semibold mb-1">{t('Your Privacy is Protected')}</p>
                <p>{t('Analytics is off by default. If you enable it, we collect ')}<strong>{t('anonymous usage data only')}</strong>{t('. No meeting content, names, file paths, or personal information is ever collected.')}</p>
              </div>
            </div>
          </div>

          <div className="border border-[var(--border-subtle)] rounded-lg p-4">
            <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('Where analytics goes')}</h4>
            <p className="text-sm text-[var(--fg2)]">
              {t('When enabled, anonymous events are sent directly to the Memento PostHog project through the US PostHog ingestion endpoint (us.i.posthog.com). They are visible to the Memento project maintainers in PostHog, not inside the meeting UI. Turning analytics off stops the client immediately.')}
            </p>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-[var(--fg1)]">{t('Data We Collect When Enabled:')}</h3>

            {/* Model Preferences */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('1. Model Preferences')}</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• {t('Transcription model (e.g., "Whisper large-v3", "Parakeet")')}</li>
                <li>• {t('Summary model (e.g., "Llama 3.2", "Claude Sonnet")')}</li>
                <li>• {t('Model provider (e.g., "Local", "Ollama", "OpenRouter")')}</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">{t('Helps us understand which models users prefer')}</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('2. Anonymous Meeting Metrics')}</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• {t('Recording duration (e.g., "125 seconds")')}</li>
                <li>• {t('Pause duration (e.g., "5 seconds")')}</li>
                <li>• {t('Number of transcript segments')}</li>
                <li>• {t('Number of audio chunks processed')}</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">{t('Helps us optimize performance and understand usage patterns')}</p>
            </div>

            {/* Device Types */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('3. Device Types (Not Names)')}</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• {t('Microphone type: "Bluetooth" or "Wired" or "Unknown"')}</li>
                <li>• {t('System audio type: "Bluetooth" or "Wired" or "Unknown"')}</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">{t('Helps us improve compatibility, NOT the actual device names')}</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('4. App Usage Patterns')}</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• {t('App started/stopped events')}</li>
                <li>• {t('Session duration')}</li>
                <li>• {t('Feature usage (e.g., "settings changed")')}</li>
                <li>• {t('Error occurrences (helps us fix bugs)')}</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">{t('Helps us improve user experience')}</p>
            </div>

            {/* Platform Info */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('5. Platform Information')}</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• {t('Operating system (e.g., "macOS", "Windows")')}</li>
                <li>• {t('App version (automatically included in all events)')}</li>
                <li>• {t('Architecture (e.g., "x86_64", "aarch64")')}</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">{t('Helps us prioritize platform support')}</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] rounded-lg p-4">
            <h4 className="font-semibold text-[var(--danger)] mb-2">{t("What We DON'T Collect:")}</h4>
            <ul className="text-sm text-[var(--danger)] space-y-1 ml-4">
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
          <div className="bg-[var(--bg-sheet)] border border-[var(--border-subtle)] rounded-lg p-4">
            <h4 className="font-semibold text-[var(--fg1)] mb-2">{t('Example Event:')}</h4>
            <pre className="text-xs text-[var(--fg2)] overflow-x-auto">
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
        <div className="flex items-center justify-end gap-4 p-6 border-t border-[var(--border-subtle)] bg-[var(--bg-sheet)]">
          <button
            onClick={onClose}
            className="px-4 py-2 text-[var(--fg2)] bg-[var(--bg-canvas)] border border-[var(--border-strong)] rounded-md hover:bg-[var(--bg-sheet)] transition-colors"
          >
            {t('Close')}
          </button>
        </div>
      </div>
    </div>
  );
}
