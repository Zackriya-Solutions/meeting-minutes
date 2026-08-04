'use client';

import React from 'react';
import { X, Info, Shield } from 'lucide-react';
import { useI18n } from '@/hooks/useI18n';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirmDisable: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose, onConfirmDisable }: AnalyticsDataModalProps) {
  const { t } = useI18n();

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-gray-200">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-blue-600" />
            <h2 className="text-xl font-semibold text-gray-900">{t('analyticsModal.title')}</h2>
          </div>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-green-50 border border-green-200 rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-green-600 mt-0.5 flex-shrink-0" />
              <div className="text-sm text-green-800">
                <p className="font-semibold mb-1">{t('analyticsModal.privacyProtectedTitle')}</p>
                <p>{t('analyticsModal.privacyProtectedBody')}</p>
              </div>
            </div>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-gray-900">{t('analyticsModal.dataWeCollectTitle')}</h3>

            {/* Model Preferences */}
            <div className="border border-gray-200 rounded-lg p-4">
              <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.modelPreferencesTitle')}</h4>
              <ul className="text-sm text-gray-700 space-y-1 ml-4">
                <li>{t('analyticsModal.modelPreferencesItem1')}</li>
                <li>{t('analyticsModal.modelPreferencesItem2')}</li>
                <li>{t('analyticsModal.modelPreferencesItem3')}</li>
              </ul>
              <p className="text-xs text-gray-500 mt-2 italic">{t('analyticsModal.modelPreferencesNote')}</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-gray-200 rounded-lg p-4">
              <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.meetingMetricsTitle')}</h4>
              <ul className="text-sm text-gray-700 space-y-1 ml-4">
                <li>{t('analyticsModal.meetingMetricsItem1')}</li>
                <li>{t('analyticsModal.meetingMetricsItem2')}</li>
                <li>{t('analyticsModal.meetingMetricsItem3')}</li>
                <li>{t('analyticsModal.meetingMetricsItem4')}</li>
              </ul>
              <p className="text-xs text-gray-500 mt-2 italic">{t('analyticsModal.meetingMetricsNote')}</p>
            </div>

            {/* Device Types */}
            <div className="border border-gray-200 rounded-lg p-4">
              <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.deviceTypesTitle')}</h4>
              <ul className="text-sm text-gray-700 space-y-1 ml-4">
                <li>{t('analyticsModal.deviceTypesItem1')}</li>
                <li>{t('analyticsModal.deviceTypesItem2')}</li>
              </ul>
              <p className="text-xs text-gray-500 mt-2 italic">{t('analyticsModal.deviceTypesNote')}</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-gray-200 rounded-lg p-4">
              <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.usagePatternsTitle')}</h4>
              <ul className="text-sm text-gray-700 space-y-1 ml-4">
                <li>{t('analyticsModal.usagePatternsItem1')}</li>
                <li>{t('analyticsModal.usagePatternsItem2')}</li>
                <li>{t('analyticsModal.usagePatternsItem3')}</li>
                <li>{t('analyticsModal.usagePatternsItem4')}</li>
              </ul>
              <p className="text-xs text-gray-500 mt-2 italic">{t('analyticsModal.usagePatternsNote')}</p>
            </div>

            {/* Platform Info */}
            <div className="border border-gray-200 rounded-lg p-4">
              <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.platformInfoTitle')}</h4>
              <ul className="text-sm text-gray-700 space-y-1 ml-4">
                <li>{t('analyticsModal.platformInfoItem1')}</li>
                <li>{t('analyticsModal.platformInfoItem2')}</li>
                <li>{t('analyticsModal.platformInfoItem3')}</li>
              </ul>
              <p className="text-xs text-gray-500 mt-2 italic">{t('analyticsModal.platformInfoNote')}</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-red-50 border border-red-200 rounded-lg p-4">
            <h4 className="font-semibold text-red-900 mb-2">{t('analyticsModal.notCollectTitle')}</h4>
            <ul className="text-sm text-red-800 space-y-1 ml-4">
              <li>{t('analyticsModal.notCollectItem1')}</li>
              <li>{t('analyticsModal.notCollectItem2')}</li>
              <li>{t('analyticsModal.notCollectItem3')}</li>
              <li>{t('analyticsModal.notCollectItem4')}</li>
              <li>{t('analyticsModal.notCollectItem5')}</li>
              <li>{t('analyticsModal.notCollectItem6')}</li>
              <li>{t('analyticsModal.notCollectItem7')}</li>
            </ul>
          </div>

          {/* Example Event */}
          <div className="bg-gray-50 border border-gray-200 rounded-lg p-4">
            <h4 className="font-semibold text-gray-900 mb-2">{t('analyticsModal.exampleEventTitle')}</h4>
            <pre className="text-xs text-gray-700 overflow-x-auto">
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
        <div className="flex items-center justify-between gap-4 p-6 border-t border-gray-200 bg-gray-50">
          <button
            onClick={onClose}
            className="px-4 py-2 text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
          >
            {t('analyticsModal.keepAnalyticsEnabled')}
          </button>
          <button
            onClick={onConfirmDisable}
            className="px-4 py-2 text-white bg-red-600 rounded-md hover:bg-red-700 transition-colors"
          >
            {t('analyticsModal.confirmDisable')}
          </button>
        </div>
      </div>
    </div>
  );
}
