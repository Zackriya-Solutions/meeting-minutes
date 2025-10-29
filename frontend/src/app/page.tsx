'use client';

import { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { RecordingControls } from '@/components/RecordingControls';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { StatusOverlays } from '@/app/_components/StatusOverlays';
import Analytics from '@/lib/analytics';
import { SettingsModals } from './_components/SettingsModal';
import { TranscriptPanel } from './_components/TranscriptPanel';
import { useModalState } from '@/hooks/useModalState';
import { useRecordingStateSync } from '@/hooks/useRecordingStateSync';
import { useRecordingStart } from '@/hooks/useRecordingStart';
import { useRecordingStop } from '@/hooks/useRecordingStop';

export default function Home() {
  // Local page state (not moved to contexts)
  const [isRecording, setIsRecordingState] = useState(false);
  const [barHeights, setBarHeights] = useState(['58%', '76%', '58%']);

  // Use contexts for state management
  const { meetingTitle } = useTranscripts();
  const { transcriptModelConfig, selectedDevices } = useConfig();
  const recordingState = useRecordingState();

  // Hooks
  const { hasMicrophone } = usePermissionCheck();
  const { setIsMeetingActive, setIsRecording: setSidebarIsRecording, isCollapsed: sidebarCollapsed } = useSidebar();
  const { modals, messages, showModal, hideModal } = useModalState(transcriptModelConfig);
  const { isRecordingDisabled, setIsRecordingDisabled } = useRecordingStateSync(isRecording, setIsRecordingState, setIsMeetingActive);
  const { handleRecordingStart } = useRecordingStart(isRecording, setIsRecordingState);
  const { handleRecordingStop, isStopping, isProcessingTranscript, isSavingTranscript, summaryStatus, setIsStopping } = useRecordingStop(
    setIsRecordingState,
    setIsRecordingDisabled
  );

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  useEffect(() => {
    if (recordingState.isRecording) {
      const interval = setInterval(() => {
        setBarHeights(prev => {
          const newHeights = [...prev];
          newHeights[0] = Math.random() * 20 + 10 + 'px';
          newHeights[1] = Math.random() * 20 + 10 + 'px';
          newHeights[2] = Math.random() * 20 + 10 + 'px';
          return newHeights;
        });
      }, 300);

      return () => clearInterval(interval);
    }
  }, [recordingState.isRecording]);

  // Update sidebar recording state when backend-synced recording state changes
  useEffect(() => {
    setSidebarIsRecording(recordingState.isRecording);
  }, [recordingState.isRecording, setSidebarIsRecording]);

  const isProcessingStop = summaryStatus === 'processing' || isProcessingTranscript;

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen bg-gray-50"
    >
      {/* All Modals supported*/}
      <SettingsModals
        modals={modals}
        messages={messages}
        onClose={hideModal}
      />
      <div className="flex flex-1 overflow-hidden">
        <TranscriptPanel
          isProcessingStop={isProcessingStop}
          isStopping={isStopping}
        />

        {/* Recording controls - only show when permissions are granted or already recording and not showing status messages */}
        {(hasMicrophone || isRecording) && !isProcessingStop && !isSavingTranscript && (
          <div className="fixed bottom-12 left-0 right-0 z-10">
            <div
              className="flex justify-center pl-8 transition-[margin] duration-300"
              style={{
                marginLeft: sidebarCollapsed ? '4rem' : '16rem'
              }}
            >
              <div className="w-2/3 max-w-[750px] flex justify-center">
                <div className="bg-white rounded-full shadow-lg flex items-center">
                  <RecordingControls
                    isRecording={recordingState.isRecording}
                    onRecordingStop={(callApi = true) => handleRecordingStop(callApi)}
                    onRecordingStart={handleRecordingStart}
                    onTranscriptReceived={() => { }} // Not actually used by RecordingControls
                    onStopInitiated={() => setIsStopping(true)}
                    barHeights={barHeights}
                    onTranscriptionError={(message) => {
                      showModal('errorAlert', message);
                    }}
                    isRecordingDisabled={isRecordingDisabled}
                    isParentProcessing={isProcessingStop}
                    selectedDevices={selectedDevices}
                    meetingName={meetingTitle}
                  />
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Status Overlays - Processing and Saving */}
        <StatusOverlays
          isProcessing={summaryStatus === 'processing' && !isRecording}
          isSaving={isSavingTranscript}
          sidebarCollapsed={sidebarCollapsed}
        />
      </div>
    </motion.div>
  );
}
