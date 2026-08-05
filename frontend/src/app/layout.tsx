'use client'

import './globals.css'
import '@/vendor/deslop/deslop-primitives.css'
import { LanguageProvider } from '@/lib/i18n'
import { SidebarProvider } from '@/components/Sidebar/SidebarProvider'
import { AppSidebar } from '@/components/AppSidebar'
import { SidebarInset, SidebarProvider as ShadcnSidebarProvider } from '@/components/ui/sidebar'
import MainContent from '@/components/MainContent'
import AnalyticsProvider from '@/components/AnalyticsProvider'
import { toast } from 'sonner'
import { Toaster } from '@/components/ui/sonner'
import "sonner/dist/styles.css"
import { useState, useEffect, useCallback } from 'react'
import { listen, UnlistenFn } from '@tauri-apps/api/event'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { TooltipProvider } from '@/components/ui/tooltip'
import { RecordingStateProvider } from '@/contexts/RecordingStateContext'
import { OllamaDownloadProvider } from '@/contexts/OllamaDownloadContext'
import { TranscriptProvider } from '@/contexts/TranscriptContext'
import { ConfigProvider } from '@/contexts/ConfigContext'
import { OnboardingProvider } from '@/contexts/OnboardingContext'
import { loadBetaFeatures } from '@/types/betaFeatures'
import { DownloadProgressToastProvider } from '@/components/shared/DownloadProgressToast'
import { UpdateCheckProvider } from '@/components/UpdateCheckProvider'
import { RecordingPostProcessingProvider } from '@/contexts/RecordingPostProcessingProvider'
import { ImportAudioDialog, ImportDropOverlay } from '@/components/ImportAudio'
import { ImportDialogProvider } from '@/contexts/ImportDialogContext'
import { isAudioExtension, getAudioFormatsDisplayList } from '@/constants/audioFormats'
import { ManagedDefaultsMigrationDialog } from '@/components/ManagedDefaultsMigrationDialog'
import { AutoMeetingDetection } from '@/components/AutoMeetingDetection'
import { ThemeProvider, useTheme } from 'next-themes'
import { GlobalRecordingPill } from '@/components/GlobalRecordingPill'

function NativeWindowThemeSync() {
  const { resolvedTheme } = useTheme()

  useEffect(() => {
    if (!isTauri() || (resolvedTheme !== 'light' && resolvedTheme !== 'dark')) return

    getCurrentWindow().setTheme(resolvedTheme).catch((error) => {
      console.warn('[Layout] Failed to sync native window theme', error)
    })
  }, [resolvedTheme])

  return null
}

const WINDOW_DRAG_BLOCK_SELECTOR = [
  'button',
  'a[href]',
  'input',
  'textarea',
  'select',
  'option',
  'label',
  'summary',
  '[contenteditable="true"]',
  '[draggable="true"]',
  '[tabindex]:not([tabindex="-1"])',
  '[role="button"]',
  '[role="link"]',
  '[role="menuitem"]',
  '[role="option"]',
  '[role="tab"]',
  '[role="switch"]',
  '[role="checkbox"]',
  '[role="radio"]',
  '[role="slider"]',
  '.no-drag',
  '.memento-drawer-swipe-handle',
  '[data-no-window-drag]',
].join(',')

function NativeWindowBackgroundDrag() {
  useEffect(() => {
    if (!isTauri()) return

    const handleMouseDown = (event: MouseEvent) => {
      if (
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.shiftKey
      ) return

      const target = event.target
      if (!(target instanceof Element) || target.closest(WINDOW_DRAG_BLOCK_SELECTOR)) return

      window.getSelection()?.removeAllRanges()
      event.preventDefault()
      getCurrentWindow().startDragging().catch((error) => {
        console.warn('[Layout] Failed to start native window drag', error)
      })
    }

    // Bubble after the target has had a chance to identify itself as an
    // interactive control. Capture-phase preventDefault can cancel the
    // browser's follow-up click in WebKit, which makes otherwise normal
    // buttons (including archive cells) appear dead.
    window.addEventListener('mousedown', handleMouseDown)
    return () => window.removeEventListener('mousedown', handleMouseDown)
  }, [])

  return null
}

// Module-level component — stable reference across RootLayout re-renders.
// Defined here (not inside RootLayout) so React never sees a new function type
// on re-render, which would cause unmount/remount and break initialization logic.
function ConditionalImportDialog({
  showImportDialog,
  handleImportDialogClose,
  importFilePath,
}: {
  showImportDialog: boolean;
  handleImportDialogClose: (open: boolean) => void;
  importFilePath: string | null;
}) {
  return (
    <ImportAudioDialog
      open={showImportDialog}
      onOpenChange={handleImportDialogClose}
      preselectedFile={importFilePath}
    />
  );
}

// export { metadata } from './metadata'

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  // Managed cloud providers make model-download onboarding unnecessary.
  const showOnboarding = false

  // Tauri's native macOS material lives behind the webview. Keep the regular
  // opaque canvas in browsers and on other desktop platforms.
  useEffect(() => {
    const usesNativeVibrancy = isTauri() && /Macintosh|Mac OS X/i.test(navigator.userAgent)
    document.documentElement.classList.toggle('native-macos-vibrancy', usesNativeVibrancy)

    return () => {
      document.documentElement.classList.remove('native-macos-vibrancy')
    }
  }, [])

  // Import audio state
  const [showDropOverlay, setShowDropOverlay] = useState(false)
  const [showImportDialog, setShowImportDialog] = useState(false)
  const [importFilePath, setImportFilePath] = useState<string | null>(null)

  // Disable context menu in production
  useEffect(() => {
    if (process.env.NODE_ENV === 'production') {
      const handleContextMenu = (e: MouseEvent) => e.preventDefault();
      document.addEventListener('contextmenu', handleContextMenu);
      return () => document.removeEventListener('contextmenu', handleContextMenu);
    }
  }, []);
  useEffect(() => {
    // Listen for tray recording toggle request
    const unlisten = listen('request-recording-toggle', () => {
      console.log('[Layout] Received request-recording-toggle from tray');

      if (showOnboarding) {
        toast.error("Сначала заверши настройку", {
          description: "Заверши первые шаги, затем начни запись."
        });
      } else {
        // If in main app, forward to useRecordingStart via window event
        console.log('[Layout] Forwarding to start-recording-from-sidebar');
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      }
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, [showOnboarding]);

  // Handle file drop for audio import
  const handleFileDrop = useCallback((paths: string[]) => {
    // Check if beta features are enabled (read from localStorage directly since we're outside ConfigProvider)
    const betaFeatures = loadBetaFeatures();

    if (!betaFeatures.importAndRetranscribe) {
      toast.error('Экспериментальная функция выключена', {
        description: 'Включи «Импорт аудио» в разделе «Настройки → Экспериментальные функции».'
      });
      return;
    }

    // Find the first audio file
    const audioFile = paths.find(p => {
      const ext = p.split('.').pop()?.toLowerCase();
      return !!ext && isAudioExtension(ext);
    });

    if (audioFile) {
      console.log('[Layout] Audio file dropped:', audioFile);
      setImportFilePath(audioFile);
      setShowImportDialog(true);
    } else if (paths.length > 0) {
      toast.error('Перетащи аудиофайл', {
        description: `Поддерживаемые форматы: ${getAudioFormatsDisplayList()}`
      });
    }
  }, []);

  // Listen for drag-drop events
  useEffect(() => {
    if (showOnboarding) return; // Don't handle drops during onboarding

    const unlisteners: UnlistenFn[] = [];
    const cleanedUpRef = { current: false };

    const setupListeners = async () => {
      // Drag enter/over - show overlay only if beta feature is enabled
      const unlistenDragEnter = await listen('tauri://drag-enter', () => {
        if (loadBetaFeatures().importAndRetranscribe) {
          setShowDropOverlay(true);
        }
      });
      if (cleanedUpRef.current) {
        unlistenDragEnter();
        return;
      }
      unlisteners.push(unlistenDragEnter);

      // Drag leave - hide overlay
      const unlistenDragLeave = await listen('tauri://drag-leave', () => {
        setShowDropOverlay(false);
      });
      if (cleanedUpRef.current) {
        unlistenDragLeave();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenDragLeave);

      // Drop - process files
      const unlistenDrop = await listen<{ paths: string[] }>('tauri://drag-drop', (event) => {
        setShowDropOverlay(false);
        handleFileDrop(event.payload.paths);
      });
      if (cleanedUpRef.current) {
        unlistenDrop();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenDrop);
    };

    setupListeners();

    return () => {
      cleanedUpRef.current = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [showOnboarding, handleFileDrop]);

  // Handle import dialog close
  const handleImportDialogClose = useCallback((open: boolean) => {
    setShowImportDialog(open);
    if (!open) {
      setImportFilePath(null);
    }
  }, []);

  // Handler for ImportDialogProvider - opens import dialog from any child component
  const handleOpenImportDialog = useCallback((filePath?: string | null) => {
    setImportFilePath(filePath ?? null);
    setShowImportDialog(true);
  }, []);

  return (
    <html lang="ru" suppressHydrationWarning>
      <body className="antialiased">
        <ThemeProvider
          attribute="class"
          defaultTheme="light"
          enableSystem={false}
          disableTransitionOnChange
          storageKey="memento-theme"
        >
          <NativeWindowThemeSync />
          <NativeWindowBackgroundDrag />
          <LanguageProvider>
            <AnalyticsProvider>
              <RecordingStateProvider>
                <TranscriptProvider>
                  <ConfigProvider>
                    <OllamaDownloadProvider>
                      <OnboardingProvider>
                        <UpdateCheckProvider>
                          <SidebarProvider>
                            <ImportDialogProvider onOpen={handleOpenImportDialog}>
                              <ShadcnSidebarProvider defaultOpen>
                                <AppSidebar />
                                <SidebarInset className="min-w-0 bg-transparent">
                              <TooltipProvider>
                                <RecordingPostProcessingProvider>
                                  {/* Download progress toast provider - listens for background downloads */}
                                  <DownloadProgressToastProvider />
                                  <ManagedDefaultsMigrationDialog />
                                  <AutoMeetingDetection />
                                  <GlobalRecordingPill />

                                  <MainContent>{children}</MainContent>
                                  {/* Import audio overlay and dialog */}
                                  <ImportDropOverlay visible={showDropOverlay} />
                                  <ConditionalImportDialog
                                    showImportDialog={showImportDialog}
                                    handleImportDialogClose={handleImportDialogClose}
                                    importFilePath={importFilePath}
                                  />
                                </RecordingPostProcessingProvider>
                              </TooltipProvider>
                                </SidebarInset>
                              </ShadcnSidebarProvider>
                            </ImportDialogProvider>
                          </SidebarProvider>
                        </UpdateCheckProvider>
                      </OnboardingProvider>
                    </OllamaDownloadProvider>
                  </ConfigProvider>
                </TranscriptProvider>
              </RecordingStateProvider>
            </AnalyticsProvider>

            <Toaster position="bottom-center" />
          </LanguageProvider>
        </ThemeProvider>
      </body>
    </html>
  )
}
