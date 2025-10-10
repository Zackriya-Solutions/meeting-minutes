'use client';

import React, { useState } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { ModelManager } from '@/components/WhisperModelManager';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';

type SettingsTab = 'general' | 'recording' | 'Transcriptionmodels';

export default function SettingsPage() {
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<SettingsTab>('general');

  const tabs = [
    { id: 'general' as const, label: 'General', icon: <Settings2 className="w-4 h-4" /> },
    { id: 'recording' as const, label: 'Recordings', icon: <Mic className="w-4 h-4" /> },
    { id: 'Transcriptionmodels' as const, label: 'Transcription', icon: <DatabaseIcon className="w-4 h-4" /> },
  ];

  return (
    <div className="min-h-screen bg-gray-50 flex flex-col">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 bg-gray-50 border-b border-gray-200">
        <div className="max-w-6xl mx-auto px-8 py-6">
          <div className="flex items-center gap-4">
            <button
              onClick={() => router.back()}
              className="flex items-center gap-2 text-gray-600 hover:text-gray-900 transition-colors"
            >
              <ArrowLeft className="w-5 h-5" />
              <span>Back</span>
            </button>
            <h1 className="text-3xl font-bold">Settings</h1>
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto p-8 pt-6">
          {/* Tabs */}
          <div className="bg-white rounded-lg shadow-sm border border-gray-200 overflow-hidden">
            <div className="flex border-b border-gray-200 overflow-x-auto">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`flex items-center gap-2 px-6 py-4 text-sm font-medium transition-colors whitespace-nowrap ${activeTab === tab.id
                      ? 'border-b-2 border-blue-600 text-blue-600 bg-blue-50'
                      : 'text-gray-600 hover:text-gray-900 hover:bg-gray-50'
                    }`}
                >
                  {tab.icon}
                  {tab.label}
                </button>
              ))}
            </div>

            {/* Tab Content */}
            <div className="p-6">
              {activeTab === 'general' && <PreferenceSettings />}
              {activeTab === 'recording' && <RecordingSettings />}
              {activeTab === 'Transcriptionmodels' && (
                <div>
                  <h3 className="text-lg font-semibold mb-4">Whisper Model Management</h3>
                  <p className="text-sm text-gray-600 mb-6">
                    Download and manage Whisper speech recognition models for local transcription.
                  </p>
                  <ModelManager
                    selectedModel=""
                    onModelSelect={(model) => console.log('Model selected:', model)}
                  />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
