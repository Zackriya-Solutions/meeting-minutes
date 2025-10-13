'use client';

import React, { useState, useEffect } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon, SparkleIcon } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { ModelManager } from '@/components/WhisperModelManager';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';

type SettingsTab = 'general' | 'recording' | 'Transcriptionmodels' | 'summaryModels';

export default function SettingsPage() {
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<SettingsTab>('general');
  const [selectedWhisperModel, setSelectedWhisperModel] = useState<string>('');
  const [isSavingModel, setIsSavingModel] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  const tabs = [
    { id: 'general' as const, label: 'General', icon: <Settings2 className="w-4 h-4" /> },
    { id: 'recording' as const, label: 'Recordings', icon: <Mic className="w-4 h-4" /> },
    { id: 'Transcriptionmodels' as const, label: 'Transcription', icon: <DatabaseIcon className="w-4 h-4" /> },
    { id: 'summaryModels' as const, label: 'Summary', icon: <SparkleIcon className="w-4 h-4" /> }
  ];

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config && config.provider === 'localWhisper' && config.model) {
          console.log('Loaded saved Whisper model:', config.model);
          setSelectedWhisperModel(config.model);
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, []);

  // Handle model selection and save to database
  const handleModelSelect = async (modelName: string) => {
    try {
      console.log('[SettingsPage] ==========================================');
      console.log('[SettingsPage] handleModelSelect called with:', modelName);
      console.log('[SettingsPage] Type of modelName:', typeof modelName);
      console.log('[SettingsPage] Updating local state...');

      setIsSavingModel(true);
      setSaveSuccess(false);
      setSelectedWhisperModel(modelName);

      const payload = {
        provider: 'localWhisper',
        model: modelName,
        apiKey: null
      };
      console.log('[SettingsPage] Calling invoke with payload:', JSON.stringify(payload));

      // Save to database
      const result = await invoke('api_save_transcript_config', payload);

      console.log('[SettingsPage] Invoke result:', result);
      console.log('[SettingsPage] ✅ Successfully saved Whisper model:', modelName);
      setSaveSuccess(true);

      // Clear success message after 3 seconds
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (error) {
      console.error('[SettingsPage] ❌ Failed to save model selection - Full error:', error);
      console.error('[SettingsPage] Error type:', typeof error);
      console.error('[SettingsPage] Error message:', error instanceof Error ? error.message : String(error));
      console.error('[SettingsPage] Error stack:', error instanceof Error ? error.stack : 'No stack');
    } finally {
      console.log('[SettingsPage] Setting isSavingModel to false');
      setIsSavingModel(false);
      console.log('[SettingsPage] ==========================================');
    }
  };

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
                  <div className="flex items-center justify-between mb-4">
                    <div>
                      <h3 className="text-lg font-semibold">Whisper Model Management</h3>
                      <p className="text-sm text-gray-600 mt-1">
                        Download and manage Whisper speech recognition models for local transcription.
                      </p>
                    </div>
                    {saveSuccess && (
                      <div className="flex items-center gap-2 text-green-600 text-sm bg-green-50 px-3 py-2 rounded-md animate-in fade-in duration-300">
                        <span className="text-lg">✓</span>
                        <span>Model saved!</span>
                      </div>
                    )}
                    {isSavingModel && (
                      <div className="flex items-center gap-2 text-blue-600 text-sm">
                        <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-600"></div>
                        <span>Saving...</span>
                      </div>
                    )}
                  </div>
                  <ModelManager
                    selectedModel={selectedWhisperModel}
                    onModelSelect={handleModelSelect}
                    autoSave={false}
                  />
                </div>
              )}
              {activeTab === 'summaryModels' && <SummaryModelSettings />}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
