'use client';

import React from 'react';
import { X, Info, Shield } from 'lucide-react';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirmDisable: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose, onConfirmDisable }: AnalyticsDataModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-overlay/50 flex items-center justify-center z-50">
      <div className="bg-card rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-info-text" />
            <h2 className="text-xl font-semibold text-foreground">What Analytics Collects</h2>
          </div>
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-muted-foreground transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-success/10 border border-success/30 rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-success-text mt-0.5 flex-shrink-0" />
              <div className="text-sm text-success-text">
                <p className="font-semibold mb-1">Your Privacy is Protected</p>
                <p>Analytics is off by default. If you enable it, we collect <strong>anonymous usage data only</strong>. No meeting content, names, file paths, or personal information is ever collected.</p>
              </div>
            </div>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-foreground">Data We Collect When Enabled:</h3>

            {/* Model Preferences */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">1. Model Preferences</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Transcription model (e.g., "Whisper large-v3", "Parakeet")</li>
                <li>• Summary model (e.g., "Llama 3.2", "Claude Sonnet")</li>
                <li>• Model provider (e.g., "Local", "Ollama", "OpenRouter")</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Helps us understand which models users prefer</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">2. Anonymous Meeting Metrics</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Recording duration (e.g., "125 seconds")</li>
                <li>• Pause duration (e.g., "5 seconds")</li>
                <li>• Number of transcript segments</li>
                <li>• Number of audio chunks processed</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Helps us optimize performance and understand usage patterns</p>
            </div>

            {/* Device Types */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">3. Device Types (Not Names)</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Microphone type: "Bluetooth" or "Wired" or "Unknown"</li>
                <li>• System audio type: "Bluetooth" or "Wired" or "Unknown"</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Helps us improve compatibility, NOT the actual device names</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">4. App Usage Patterns</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• App started/stopped events</li>
                <li>• Session duration</li>
                <li>• Feature usage (e.g., "settings changed")</li>
                <li>• Error occurrences (helps us fix bugs)</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Helps us improve user experience</p>
            </div>

            {/* Platform Info */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">5. Platform Information</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Operating system (e.g., "macOS", "Windows")</li>
                <li>• App version (automatically included in all events)</li>
                <li>• Architecture (e.g., "x86_64", "aarch64")</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Helps us prioritize platform support</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-4">
            <h4 className="font-semibold text-destructive mb-2">What We DON'T Collect:</h4>
            <ul className="text-sm text-destructive space-y-1 ml-4">
              <li>• ❌ Meeting names or titles</li>
              <li>• ❌ File names, file paths, or meeting folders</li>
              <li>• ❌ Meeting transcripts or content</li>
              <li>• ❌ Audio recordings</li>
              <li>• ❌ Device names (only types: Bluetooth/Wired)</li>
              <li>• ❌ Personal information</li>
              <li>• ❌ Any identifiable data</li>
            </ul>
          </div>

          {/* Example Event */}
          <div className="bg-muted/40 border border-border rounded-lg p-4">
            <h4 className="font-semibold text-foreground mb-2">Example Event:</h4>
            <pre className="text-xs text-foreground overflow-x-auto">
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
        <div className="flex items-center justify-between gap-4 p-6 border-t border-border bg-muted/40">
          <button
            onClick={onClose}
            className="px-4 py-2 text-foreground bg-card border border-border rounded-md hover:bg-muted/40 transition-colors"
          >
            Keep Analytics Enabled
          </button>
          <button
            onClick={onConfirmDisable}
            className="px-4 py-2 text-destructive-foreground bg-destructive rounded-md hover:bg-destructive/90 transition-colors"
          >
            Confirm: Disable Analytics
          </button>
        </div>
      </div>
    </div>
  );
}
