'use client'

import './globals.css'
import Sidebar from '@/components/Sidebar'
import { SidebarProvider } from '@/components/Sidebar/SidebarProvider'
import MainContent from '@/components/MainContent'
import AnalyticsProvider from '@/components/AnalyticsProvider'
import { Toaster, toast } from 'sonner'
import "sonner/dist/styles.css"
import { useState, useEffect, useCallback } from 'react'
import { listen, UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { TooltipProvider } from '@/components/ui/tooltip'
import { RecordingStateProvider } from '@/contexts/RecordingStateContext'
import { OllamaDownloadProvider } from '@/contexts/OllamaDownloadContext'
import { TranscriptProvider } from '@/contexts/TranscriptContext'
import { ConfigProvider, useConfig } from '@/contexts/ConfigContext'
import { OnboardingProvider } from '@/contexts/OnboardingContext'
import { OnboardingFlow } from '@/components/onboarding'
import { loadBetaFeatures } from '@/types/betaFeatures'
import { DownloadProgressToastProvider } from '@/components/shared/DownloadProgressToast'
import { UpdateCheckProvider } from '@/components/UpdateCheckProvider'
import { RecordingPostProcessingProvider } from '@/contexts/RecordingPostProcessingProvider'
import { ImportAudioDialog, ImportDropOverlay } from '@/components/ImportAudio'
import { OrphanCheckpointListener } from '@/components/OrphanCheckpointListener'
import { ImportDialogProvider } from '@/contexts/ImportDialogContext'
import { isAudioExtension, getAudioFormatsDisplayList } from '@/constants/audioFormats'

function ConditionalImportDialog({
  showImportDialog,
  handleImportDialogClose,
  importFilePath,
}: {
  showImportDialog: boolean;
  handleImportDialogClose: (open: boolean) => void;
  importFilePath: string | null;
}) {
  const { betaFeatures } = useConfig();
  if (!betaFeatures.importAndRetranscribe) return null;
  return (
    <ImportAudioDialog
      open={showImportDialog}
      onOpenChange={handleImportDialogClose}
      preselectedFile={importFilePath}
    />
  );
}

export default function ClientRootLayout({ children }: { children: React.ReactNode }) {
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [, setOnboardingCompleted] = useState(false);
  const [showDropOverlay, setShowDropOverlay] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [importFilePath, setImportFilePath] = useState<string | null>(null);

  useEffect(() => {
    invoke<{ completed: boolean } | null>('get_onboarding_status')
      .then((status) => {
        const isComplete = status?.completed ?? false;
        setOnboardingCompleted(isComplete);
        if (!isComplete) {
          setShowOnboarding(true);
        }
      })
      .catch(() => {
        setShowOnboarding(true);
        setOnboardingCompleted(false);
      });
  }, []);

  useEffect(() => {
    if (process.env.NODE_ENV === 'production') {
      const handler = (e: MouseEvent) => e.preventDefault();
      document.addEventListener('contextmenu', handler);
      return () => document.removeEventListener('contextmenu', handler);
    }
  }, []);

  useEffect(() => {
    const unlisten = listen('request-recording-toggle', () => {
      if (showOnboarding) {
        toast.error('Please complete setup first', {
          description: 'You need to finish onboarding before you can start recording.'
        });
      } else {
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, [showOnboarding]);

  const handleFileDrop = useCallback((paths: string[]) => {
    const audioFile = paths.find((p) => {
      const ext = p.split('.').pop()?.toLowerCase();
      return !!ext && isAudioExtension(ext);
    });
    if (audioFile) {
      setImportFilePath(audioFile);
      setShowImportDialog(true);
    } else if (paths.length > 0) {
      toast.error('Please drop an audio file', {
        description: `Supported formats: ${getAudioFormatsDisplayList()}`
      });
    }
  }, []);

  useEffect(() => {
    if (showOnboarding) return;
    const unlisteners: UnlistenFn[] = [];
    const cleanedUpRef = { current: false };
    const setupListeners = async () => {
      const unlistenDragEnter = await listen('tauri://drag-enter', () => {
        if (loadBetaFeatures().importAndRetranscribe) setShowDropOverlay(true);
      });
      if (cleanedUpRef.current) { unlistenDragEnter(); return; }
      unlisteners.push(unlistenDragEnter);

      const unlistenDragLeave = await listen('tauri://drag-leave', () => {
        setShowDropOverlay(false);
      });
      if (cleanedUpRef.current) { unlistenDragLeave(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unlistenDragLeave);

      const unlistenDrop = await listen<{ paths: string[] }>('tauri://drag-drop', (event) => {
        setShowDropOverlay(false);
        handleFileDrop(event.payload.paths);
      });
      if (cleanedUpRef.current) { unlistenDrop(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unlistenDrop);
    };
    setupListeners();
    return () => {
      cleanedUpRef.current = true;
      unlisteners.forEach(u => u());
    };
  }, [showOnboarding, handleFileDrop]);

  const handleImportDialogClose = useCallback((open: boolean) => {
    setShowImportDialog(open);
    if (!open) setImportFilePath(null);
  }, []);

  const handleOpenImportDialog = useCallback((filePath?: string | null) => {
    setImportFilePath(filePath ?? null);
    setShowImportDialog(true);
  }, []);

  const handleOnboardingComplete = () => {
    setShowOnboarding(false);
    window.location.reload();
  };

  return (
    <AnalyticsProvider>
      <RecordingStateProvider>
        <TranscriptProvider>
          <ConfigProvider>
            <OllamaDownloadProvider>
              <OnboardingProvider>
                <UpdateCheckProvider>
                  <SidebarProvider>
                    <TooltipProvider>
                      <RecordingPostProcessingProvider>
                        <ImportDialogProvider onOpen={handleOpenImportDialog}>
                          <DownloadProgressToastProvider />
                          {showOnboarding ? (
                            <OnboardingFlow onComplete={handleOnboardingComplete} />
                          ) : (
                            <div className="flex">
                              <Sidebar />
                              <MainContent>{children}</MainContent>
                            </div>
                          )}
                          <ImportDropOverlay visible={showDropOverlay} />
                          <ConditionalImportDialog
                            showImportDialog={showImportDialog}
                            handleImportDialogClose={handleImportDialogClose}
                            importFilePath={importFilePath}
                          />
                          <OrphanCheckpointListener />
                        </ImportDialogProvider>
                      </RecordingPostProcessingProvider>
                    </TooltipProvider>
                  </SidebarProvider>
                </UpdateCheckProvider>
              </OnboardingProvider>
            </OllamaDownloadProvider>
          </ConfigProvider>
        </TranscriptProvider>
      </RecordingStateProvider>
    </AnalyticsProvider>
  );
}