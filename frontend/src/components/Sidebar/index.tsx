'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import { ChevronDown, ChevronRight, File, Settings, ChevronLeftCircle, ChevronRightCircle, Calendar, StickyNote, Home, Trash2, Mic, Square, Plus, Search, Pencil, NotebookPen, SearchIcon, X, Upload, MessageSquare } from '@/components/memento/LucideCompat';
import { useRouter, usePathname } from 'next/navigation';
import {
  useSidebar,
  persistSidebarWidth,
  clearStoredSidebarWidth,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_MAX_WIDTH,
} from './SidebarProvider';
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { ConfirmationModal } from '../ConfirmationModel/confirmation-modal';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '../SettingTabs';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useLanguage } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';

import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"

import { MessageToast } from '../MessageToast';
import Logo from '../Logo';
import Info from '../Info';
import { ComplianceNotification } from '../ComplianceNotification';
import { Input } from '../ui/input';
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from '../ui/input-group';
import { Icon as MementoIcon } from '../memento/Icon';

interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  createdAt?: string | null;
  occurredAt?: string | null;
  folderPath?: string | null;
  children?: SidebarItem[];
}

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const {
    currentMeeting,
    setCurrentMeeting,
    sidebarItems,
    isCollapsed,
    toggleCollapse,
    sidebarWidth,
    setSidebarWidth,
    isSidebarResizing,
    setIsSidebarResizing,
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    setMeetings,
    serverAddress
  } = useSidebar();

  // Right-edge drag handle: pointer capture keeps the drag alive over the main
  // content; the sidebar is fixed at the window's left edge, so clientX is the
  // desired width directly.
  const handleSidebarResizeStart = (e: React.PointerEvent<HTMLDivElement>) => {
    if (isCollapsed) return;
    e.preventDefault();
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    setIsSidebarResizing(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';

    let width = sidebarWidth;
    const onMove = (ev: PointerEvent) => {
      width = Math.min(Math.max(ev.clientX, SIDEBAR_MIN_WIDTH), SIDEBAR_MAX_WIDTH);
      setSidebarWidth(width);
    };
    const onEnd = () => {
      handle.removeEventListener('pointermove', onMove);
      handle.removeEventListener('pointerup', onEnd);
      handle.removeEventListener('pointercancel', onEnd);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      setIsSidebarResizing(false);
      persistSidebarWidth(width);
    };
    handle.addEventListener('pointermove', onMove);
    handle.addEventListener('pointerup', onEnd);
    handle.addEventListener('pointercancel', onEnd);
  };

  // Double-click restores the default width.
  const resetSidebarWidth = () => {
    setSidebarWidth(SIDEBAR_DEFAULT_WIDTH);
    clearStoredSidebarWidth();
  };

  // Get recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();
  const { t, lang } = useLanguage();
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['meetings']));
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: '',
    whisperModel: '',
    apiKey: null,
    ollamaEndpoint: null
  });
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
  });
  const [settingsSaveSuccess, setSettingsSaveSuccess] = useState<boolean | null>(null);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      const key = event.key.toLowerCase();
      if (!['r', 'k', 'm'].includes(key)) return;
      event.preventDefault();

      if (key === 'k') {
        router.push('/search');
      } else if (key === 'r' && !isRecording) {
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else if (key === 'm' && isRecording) {
        window.dispatchEvent(new CustomEvent('memento-mark-moment'));
      }
    };

    window.addEventListener('keydown', handleShortcut);
    return () => window.removeEventListener('keydown', handleShortcut);
  }, [isRecording, router]);

  // State for edit modal
  const [editModalState, setEditModalState] = useState<{ isOpen: boolean; meetingId: string | null; currentTitle: string }>({
    isOpen: false,
    meetingId: null,
    currentTitle: ''
  });
  const [editingTitle, setEditingTitle] = useState<string>('');
  const [sourceTitle, setSourceTitle] = useState<{ meetingId: string; title: string } | null>(null);

  // Ensure 'meetings' folder is always expanded
  useEffect(() => {
    if (!expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders]);

  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000);
  //   }
  // }, [settingsSaveSuccess]);


  const [deleteModalState, setDeleteModalState] = useState<{ isOpen: boolean; itemId: string | null }>({ isOpen: false, itemId: null });

  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchModelConfig = async () => {
      // Only make API call if serverAddress is loaded
      if (!serverAddress) {
        console.log('Waiting for server address to load before fetching model config');
        return;
      }

      try {
        const data = await invoke('api_get_model_config') as any;
        if (data && data.provider !== null) {
          // Fetch API key if not included and provider requires it
          if (data.provider !== 'ollama' && !data.apiKey) {
            try {
              const apiKeyData = await invoke('api_get_api_key', {
                provider: data.provider
              }) as string;
              data.apiKey = apiKeyData;
            } catch (err) {
              console.error('Failed to fetch API key:', err);
            }
          }
          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
    };

    fetchModelConfig();
  }, [serverAddress]);


  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchTranscriptSettings = async () => {
      // Only make API call if serverAddress is loaded
      if (!serverAddress) {
        console.log('Waiting for server address to load before fetching transcript settings');
        return;
      }

      try {
        const data = await invoke('api_get_transcript_config') as any;
        if (data && data.provider !== null) {
          setTranscriptModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch transcript settings:', error);
      }
    };
    fetchTranscriptSettings();
  }, [serverAddress]);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('Sidebar received model-config-updated event:', event.payload);
        setModelConfig(event.payload);
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);



  // Handle model config save
  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey,
        ollamaEndpoint: config.ollamaEndpoint,
      });

      setModelConfig(config);
      console.log('Model config saved successfully');
      setSettingsSaveSuccess(true);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      // Track settings change
      await Analytics.trackSettingsChanged('model_config', `${config.provider}_${config.model}`);
    } catch (error) {
      console.error('Error saving model config:', error);
      setSettingsSaveSuccess(false);
    }
  };

  const handleSaveTranscriptConfig = async (updatedConfig?: TranscriptModelProps) => {
    try {
      const configToSave = updatedConfig || transcriptModelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        apiKey: configToSave.apiKey ?? null
      };
      console.log('Saving transcript config with payload:', payload);

      await invoke('api_save_transcript_config', {
        provider: payload.provider,
        model: payload.model,
        apiKey: payload.apiKey,
      });


      setSettingsSaveSuccess(true);

      // Track settings change
      const transcriptConfigToSave = updatedConfig || transcriptModelConfig;
      await Analytics.trackSettingsChanged('transcript_config', `${transcriptConfigToSave.provider}_${transcriptConfigToSave.model}`);
    } catch (error) {
      console.error('Failed to save transcript config:', error);
      setSettingsSaveSuccess(false);
    }
  };

  // Handle search input changes
  const handleSearchChange = useCallback((value: string) => {
    setSearchQuery(value);

    // Make sure the meetings folder is expanded when searching
    if (value.trim() && !expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders]);

  useEffect(() => {
    if (!searchQuery.trim()) {
      searchTranscripts('');
      return;
    }
    const timer = window.setTimeout(() => {
      searchTranscripts(searchQuery);
    }, 250);
    return () => window.clearTimeout(timer);
  }, [searchQuery, searchTranscripts]);

  // Combine search results with sidebar items
  const filteredSidebarItems = useMemo(() => {
    if (!searchQuery.trim()) return sidebarItems;
    const normalizedQuery = searchQuery.toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US');
    const titleMatches = (item: SidebarItem) => {
      const displayTitle = getMeetingDisplayInfo(item, lang).title;
      return (
        item.title.toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US').includes(normalizedQuery)
        || displayTitle.toLocaleLowerCase(lang === 'ru' ? 'ru-RU' : 'en-US').includes(normalizedQuery)
      );
    };

    // If we have search results, highlight matching meetings
    if (searchResults.length > 0) {
      // Get the IDs of meetings that matched in transcripts
      const matchedMeetingIds = new Set(searchResults.map(result => result.id));

      return sidebarItems
        .map(folder => {
          // Always include folders in the results
          if (folder.type === 'folder') {
            if (!folder.children) return folder;

            // Filter children based on search results or title match
            const filteredChildren = folder.children.filter(item => {
              // Include if the meeting ID is in our search results
              if (matchedMeetingIds.has(item.id)) return true;

              // Or if the title matches the search query
              return titleMatches(item);
            });

            return {
              ...folder,
              children: filteredChildren
            };
          }

          // For non-folder items, check if they match the search
          return (matchedMeetingIds.has(folder.id) ||
            titleMatches(folder))
            ? folder : undefined;
        })
        .filter((item): item is SidebarItem => item !== undefined); // Type-safe filter
    } else {
      // Fall back to title-only filtering if no transcript results
      return sidebarItems
        .map(folder => {
          // Always include folders in the results
          if (folder.type === 'folder') {
            if (!folder.children) return folder;

            // Filter children based on search query
            const filteredChildren = folder.children.filter(item =>
              titleMatches(item)
            );

            return {
              ...folder,
              children: filteredChildren
            };
          }

          // For non-folder items, check if they match the search
          return titleMatches(folder) ? folder : undefined;
        })
        .filter((item): item is SidebarItem => item !== undefined); // Type-safe filter
    }
  }, [sidebarItems, searchQuery, searchResults, lang]);


  const handleDelete = async (itemId: string) => {
    console.log('Deleting item:', itemId);
    const payload = {
      meetingId: itemId
    };

    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('api_delete_meeting', {
        meetingId: itemId,
        deleteRecordingFiles: false,
      });
      console.log('Встреча удалена');
      const updatedMeetings = meetings.filter((m: CurrentMeeting) => m.id !== itemId);
      setMeetings(updatedMeetings);

      // Track meeting deletion
      Analytics.trackMeetingDeleted(itemId);

      // Show success toast
      toast.success(t("Meeting deleted successfully"), {
        description: t("The recording folder and audio files were kept on this Mac.")
      });

      // If deleting the active meeting, navigate to home
      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
        router.push('/');
      }
    } catch (error) {
      console.error('Не удалось удалить встречу:', error);
      toast.error(t("Failed to delete meeting"), {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleDeleteConfirm = () => {
    if (deleteModalState.itemId) {
      handleDelete(deleteModalState.itemId);
    }
    setDeleteModalState({ isOpen: false, itemId: null });
  };

  // Handle modal editing of meeting names
  const handleEditStart = (meetingId: string, currentTitle: string) => {
    setEditModalState({
      isOpen: true,
      meetingId: meetingId,
      currentTitle: currentTitle
    });
    setEditingTitle(currentTitle);
    setSourceTitle(null);
    void invoke<string | null>('get_meeting_source_title', { meetingId })
      .then((title) => {
        if (title?.trim()) setSourceTitle({ meetingId, title: title.trim() });
      })
      .catch((error) => console.warn('Could not load the original meeting title:', error));
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    const meetingId = editModalState.meetingId;

    if (!meetingId) return;

    // Prevent empty titles
    if (!newTitle) {
      toast.error(t("Meeting title cannot be empty"));
      return;
    }

    try {
      await invoke('api_save_meeting_title', {
        meetingId: meetingId,
        title: newTitle,
      });

      // Update local state
      const updatedMeetings = meetings.map((m: CurrentMeeting) =>
        m.id === meetingId ? { ...m, title: newTitle } : m
      );
      setMeetings(updatedMeetings);

      // Update current meeting if it's the one being edited
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }

      // Track the edit
      Analytics.trackButtonClick('edit_meeting_title', 'sidebar');

      toast.success(t("Meeting title updated successfully"));

      // Close modal and reset state
      setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
      setEditingTitle('');
    } catch (error) {
      console.error('Не удалось сохранить название встречи:', error);
      toast.error(t("Failed to update meeting title"), {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleEditCancel = () => {
    setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
    setEditingTitle('');
    setSourceTitle(null);
  };

  const toggleFolder = (folderId: string) => {
    // Normal toggle behavior for all folders
    const newExpanded = new Set(expandedFolders);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    setExpandedFolders(newExpanded);
  };

  // Expose setShowModelSettings to window for Rust tray to call
  useEffect(() => {
    (window as any).openSettings = () => {
      setShowModelSettings(true);
    };

    // Cleanup on unmount
    return () => {
      delete (window as any).openSettings;
    };
  }, []);

  const renderCollapsedIcons = () => {
    if (!isCollapsed) return null;

    const isHomePage = pathname === '/';
    const isMeetingPage = pathname?.includes('/meeting-details');
    const isSettingsPage = pathname === '/settings';
    const isChatPage = pathname === '/chat';
    const isSearchPage = pathname === '/search';
    const isCollectionsPage = pathname === '/collections';

    return (
      <TooltipProvider>
        <div className="flex flex-col items-center space-y-4 mt-4">
          <Logo isCollapsed={isCollapsed} />

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isHomePage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="home" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Home')}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={handleRecordingToggle}
                disabled={isRecording}
                className={`p-2 text-[var(--fg-inverse)] ${isRecording ? 'bg-[var(--danger)] cursor-not-allowed' : 'bg-[var(--gold)] hover:bg-[var(--gold-active)]'} rounded-full transition-colors duration-150 shadow-none`}
              >
                {isRecording ? (
                  <MementoIcon name="stop" size={20} />
                ) : (
                  <MementoIcon name="mic" size={20} />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{isRecording ? t('Recording in progress...') : t('Start Recording')}</p>
            </TooltipContent>
          </Tooltip>

          {betaFeatures.importAndRetranscribe && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => openImportDialog()}
                  className="p-2 rounded-lg transition-colors duration-150 hover:bg-[var(--gold-soft-strong)] bg-[var(--gold-soft)]"
                >
                  <MementoIcon name="upload" size={20} />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>{t('Import Audio')}</p>
              </TooltipContent>
            </Tooltip>
          )}

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => {
                  if (isCollapsed) toggleCollapse();
                  toggleFolder('meetings');
                }}
                className={`p-2 rounded-lg transition-colors duration-150 ${isMeetingPage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="transcript" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Meeting Notes')}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/collections')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isCollectionsPage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="folder" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Collections')}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/search')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isSearchPage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="search" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Search meetings')}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/chat')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isChatPage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="chat" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Chat with archive')}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/settings')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isSettingsPage ? 'bg-[var(--bg-elevated)]' : 'hover:bg-[var(--bg-elevated)]'
                  }`}
              >
                <MementoIcon name="settings" size={20} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{t('Settings')}</p>
            </TooltipContent>
          </Tooltip>

          <Info isCollapsed={isCollapsed} />
        </div>
      </TooltipProvider>
    );
  };

  // Find matching transcript snippet for a meeting item
  const findMatchingSnippet = (itemId: string) => {
    if (!searchQuery.trim() || !searchResults.length) return null;
    return searchResults.find(result => result.id === itemId);
  };

  const renderItem = (item: SidebarItem, depth = 0) => {
    const isExpanded = expandedFolders.has(item.id);
    const paddingLeft = `${depth * 12 + 12}px`;
    const isActive = item.type === 'file' && currentMeeting?.id === item.id;
    const isMeetingItem = item.id.includes('-') && !item.id.startsWith('intro-call');
    const displayInfo = isMeetingItem
      ? getMeetingDisplayInfo(item, lang)
      : { title: item.title, dateLabel: '', dateUnknown: false };

    // Check if this item has a matching transcript snippet
    const matchingResult = isMeetingItem ? findMatchingSnippet(item.id) : null;
    const hasTranscriptMatch = !!matchingResult;

    if (isCollapsed) return null;

    return (
      <div key={item.id}>
        <div
          className={`flex items-center transition-all duration-150 group ${item.type === 'folder' && depth === 0
            ? 'p-3 text-lg font-semibold h-10 mx-3 mt-3 rounded-lg'
            : `px-3 py-2 my-0.5 rounded-md text-sm border-l-2 ${isActive ? 'border-[var(--gold)] bg-[var(--bg-elevated)] text-[var(--fg1)] font-medium' :
              hasTranscriptMatch ? 'border-transparent bg-[var(--gold-soft)]' : 'border-transparent hover:bg-[var(--bg-sheet)]'
            } cursor-pointer`
            }`}
          style={item.type === 'folder' && depth === 0 ? {} : { paddingLeft }}
          onClick={() => {
            if (item.type === 'folder') {
              toggleFolder(item.id);
            } else {
              setCurrentMeeting({
                id: item.id,
                title: item.title,
                createdAt: item.createdAt,
                occurredAt: item.occurredAt,
                folderPath: item.folderPath,
              });
              const basePath = item.id.startsWith('intro-call') ? '/' :
                item.id.includes('-') ? `/meeting-details?id=${item.id}` : `/notes/${item.id}`;
              router.push(basePath);
            }
          }}
        >
          {item.type === 'folder' ? (
            <>
              {item.id === 'meetings' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : item.id === 'notes' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : null}
              <span className={depth === 0 ? "" : "font-medium"}>{item.title}</span>
              <div className="ml-auto">
                {isExpanded ? (
                  <ChevronDown className="w-4 h-4 text-[var(--fg2)]" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-[var(--fg2)]" />
                )}
              </div>
              {searchQuery && item.id === 'meetings' && isSearching && (
                <span className="ml-2 text-xs text-[var(--gold)] animate-pulse">{t('Searching...')}</span>
              )}
            </>
          ) : (
            <div className="flex flex-col w-full">
              <div className="flex items-center w-full">
                {isMeetingItem ? (
                  <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-[var(--bg-elevated)]">
                    <File className="w-3.5 h-3.5 text-[var(--fg2)]" />
                  </div>
                ) : (
                  <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-[var(--gold-soft)]">
                    <Plus className="w-3.5 h-3.5 text-[var(--gold)]" />
                  </div>
                )}
                <span
                  className="min-w-0 flex-1"
                  title={displayInfo.title !== item.title ? item.title : undefined}
                >
                  <span className="block line-clamp-2 break-words leading-snug">
                    {displayInfo.title}
                  </span>
                  {isMeetingItem && (
                    <span className={`mt-1 block text-xs font-normal ${displayInfo.dateUnknown ? 'text-[var(--gold)]' : 'text-[var(--fg3)]'}`}>
                      {displayInfo.dateLabel}
                    </span>
                  )}
                </span>
                {isMeetingItem && (
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleEditStart(item.id, item.title);
                      }}
                      className="hover:text-[var(--gold)] p-1 rounded-md hover:bg-[var(--gold-soft)] flex-shrink-0"
                      aria-label={t("Edit meeting title")}
                    >
                      <Pencil className="w-4 h-4" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteModalState({ isOpen: true, itemId: item.id });
                      }}
                      className="hover:text-[var(--danger)] p-1 rounded-md hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] flex-shrink-0"
                      aria-label={t("Delete meeting")}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                )}
              </div>

              {/* Show transcript match snippet if available */}
              {hasTranscriptMatch && (
                <div className="mt-1 ml-8 text-xs text-[var(--fg2)] bg-[var(--gold-soft)] p-1.5 rounded border border-[var(--gold-border)] line-clamp-2">
                  <span className="font-medium text-[var(--gold)]">{t('Match:')}</span> {matchingResult.matchContext}
                </div>
              )}
            </div>
          )}
        </div>
        {item.type === 'folder' && isExpanded && item.children && (
          <div className="ml-1">
            {item.children.map(child => renderItem(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  // Date bucket (3a): 0 = Today, 1 = Yesterday, 2 = Earlier (also undated).
  const meetingBucket = (item: SidebarItem): number => {
    const raw = item.occurredAt ?? item.createdAt;
    if (!raw) return 2;
    const d = new Date(raw);
    if (Number.isNaN(d.getTime())) return 2;
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const startOfYesterday = new Date(startOfToday);
    startOfYesterday.setDate(startOfToday.getDate() - 1);
    if (d >= startOfToday) return 0;
    if (d >= startOfYesterday) return 1;
    return 2;
  };

  // 3a sidebar: meetings grouped by date (Today / Yesterday / Earlier), past groups
  // dimmed. Non-meeting rows (e.g. "+ New Call") render first, ungrouped.
  const renderGroupedMeetings = (children: SidebarItem[]) => {
    const isMeeting = (c: SidebarItem) => c.id.includes('-') && !c.id.startsWith('intro-call');
    const nonMeetings = children.filter((c) => !isMeeting(c));
    const groups: { key: string; label: string; items: SidebarItem[] }[] = [
      { key: 'today', label: t('Today'), items: [] },
      { key: 'yesterday', label: t('Yesterday'), items: [] },
      { key: 'earlier', label: t('Earlier'), items: [] },
    ];
    for (const c of children) {
      if (isMeeting(c)) groups[meetingBucket(c)].items.push(c);
    }
    return (
      <>
        {nonMeetings.map((c) => renderItem(c, 1))}
        {groups
          .filter((g) => g.items.length > 0)
          .map((g) => (
            <div key={g.key} className={g.key === 'earlier' ? 'opacity-70' : ''}>
              <div className="px-3 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--fg3)]">
                {g.label}
              </div>
              {g.items.map((c) => renderItem(c, 1))}
            </div>
          ))}
      </>
    );
  };

  // Compact icon nav for the sidebar footer (3a).
  const navIcon = (name: React.ComponentProps<typeof MementoIcon>['name'], label: string, onClick: () => void, active: boolean) => (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`flex h-9 flex-1 items-center justify-center rounded-[10px] transition-colors ${
        active
          ? 'bg-[var(--gold-soft)] text-[var(--gold)]'
          : 'bg-[var(--bg-elevated)] text-[var(--fg2)] hover:text-[var(--fg1)]'
      }`}
    >
      <MementoIcon name={name} size={17} />
    </button>
  );

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      {/* Floating collapse button */}
      <button
        onClick={toggleCollapse}
        className="absolute -right-6 top-20 z-50 p-1 bg-[var(--bg-canvas)] hover:bg-[var(--bg-elevated)] rounded-full shadow-none border"
        style={{ transform: 'translateX(50%)' }}
      >
        {isCollapsed ? (
          <ChevronRightCircle className="w-6 h-6" />
        ) : (
          <ChevronLeftCircle className="w-6 h-6" />
        )}
      </button>

      <div
        className={`memento-sidebar relative h-screen border-r flex flex-col ${isSidebarResizing ? '' : 'transition-all duration-300'} ${isCollapsed ? 'w-16' : ''
          }`}
        style={isCollapsed ? undefined : { width: sidebarWidth }}
      >
        {/* Resize handle on the right edge (expanded only). z-40 keeps it below
            the floating collapse chevron (z-50) where they overlap. */}
        {!isCollapsed && (
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label={t('Resize sidebar')}
            title={t('Resize sidebar')}
            onPointerDown={handleSidebarResizeStart}
            onDoubleClick={resetSidebarWidth}
            className="group absolute inset-y-0 -right-[3px] z-40 w-[6px] cursor-col-resize touch-none"
          >
            <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-all group-hover:w-[3px] group-hover:bg-[var(--gold-border)]" />
          </div>
        )}
        {/*  Header with traffic light spacing */}
        <div className="flex-shrink-0 h-22 flex items-center">

          {/* Title container */}



          <div className="flex-1">
            {!isCollapsed && (
              <div className="p-3">
                {/* <span className="text-lg text-center border rounded-full bg-[var(--gold-soft)] border-white font-semibold text-[var(--fg2)] mb-2 block items-center">
                  <span>Meetily</span>
                </span> */}
                <Logo isCollapsed={isCollapsed} />

                <div className="relative mb-1">
                  {/* Explicit memento tokens: the group's default border-input is nearly
                      invisible on the sheet background. */}
                  <InputGroup className="rounded-[10px] border-[var(--border-strong)] bg-[var(--surface-input)]">
                    <InputGroupInput placeholder={t('Search meeting content...')} value={searchQuery}
                      onChange={(e) => handleSearchChange(e.target.value)}
                    />
                    <InputGroupAddon>
                      <SearchIcon />
                    </InputGroupAddon>
                    {searchQuery &&
                      <InputGroupAddon align={'inline-end'}>
                        <InputGroupButton
                          onClick={() => handleSearchChange('')}
                        >
                          <X />
                        </InputGroupButton>
                      </InputGroupAddon>
                    }
                  </InputGroup>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Main content - scrollable area */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Fixed navigation items */}
          <div className="flex-shrink-0">
            {!isCollapsed && (
              <div
                onClick={() => router.push('/')}
                className={`memento-nav-item items-center h-10 flex gap-2.5 mx-3 mt-3 cursor-pointer ${pathname === '/' ? 'is-active' : ''}`}
              >
                <MementoIcon name="home" size={17} />
                <span>{t('Home')}</span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0">
            {renderCollapsedIcons()}

            {/* Scrollable meeting items (grouped by date; group headers replace the
                former "Meeting Notes" folder header) */}
            {!isCollapsed && (
              <div className="flex-1 overflow-y-auto custom-scrollbar min-h-0">
                {filteredSidebarItems
                  .filter(item => item.type === 'folder' && expandedFolders.has(item.id) && item.children)
                  .map(item => (
                    <div key={`${item.id}-children`} className="mx-3">
                      {item.id === 'meetings'
                        ? renderGroupedMeetings(item.children!)
                        : item.children!.map(child => renderItem(child, 1))}
                    </div>
                  ))}
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        {!isCollapsed && (

          <div className="flex-shrink-0 border-t border-[var(--border-subtle)] p-2.5">
            {/* Compact icon nav (3a) — collections · search · settings · chat */}
            <div className="mb-2 flex gap-1.5">
              {navIcon('folder', t('Collections'), () => router.push('/collections'), pathname === '/collections')}
              {navIcon('search', t('Search meetings'), () => router.push('/search'), pathname === '/search')}
              {navIcon('settings', t('Settings'), () => router.push('/settings'), pathname === '/settings')}
              {navIcon('chat', t('Chat with archive'), () => router.push('/chat'), pathname === '/chat')}
              {betaFeatures.importAndRetranscribe && navIcon('upload', t('Import Audio'), () => openImportDialog(), false)}
            </div>

            {/* Start recording — neutral pill with a red dot indicator */}
            <button
              onClick={handleRecordingToggle}
              disabled={isRecording}
              className={`flex w-full items-center justify-center gap-2 rounded-full border border-[var(--border-strong)] bg-[var(--bg-elevated)] px-3 py-2.5 text-sm font-semibold text-[var(--fg1)] transition-colors ${isRecording ? 'cursor-not-allowed opacity-60' : 'hover:border-[var(--gold-border)] hover:bg-[var(--gold-soft)]'}`}
            >
              {isRecording ? (
                <>
                  <MementoIcon name="stop" size={16} />
                  <span>{t('Recording in progress...')}</span>
                </>
              ) : (
                <>
                  <span className="h-2 w-2 shrink-0 rounded-full bg-[var(--danger)]" />
                  <span>{t('Start Recording')}</span>
                </>
              )}
            </button>
          </div>
        )}
      </div>

      {/* Confirmation Modal for Delete */}
      <ConfirmationModal
        isOpen={deleteModalState.isOpen}
        text={t("Are you sure you want to delete this meeting? This action cannot be undone.")}
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteModalState({ isOpen: false, itemId: null })}
      />

      {/* Изменить название встречи Modal */}
      <Dialog open={editModalState.isOpen} onOpenChange={(open) => {
        if (!open) handleEditCancel();
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>{t('Edit Meeting Title')}</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">{t('Edit Meeting Title')}</h3>
            <div className="space-y-4">
              <div>
                <label htmlFor="meeting-title" className="block text-sm font-medium text-[var(--fg2)] mb-2">
                  {t('Meeting Title')}
                </label>
                <input
                  id="meeting-title"
                  type="text"
                  value={editingTitle}
                  onChange={(e) => setEditingTitle(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      handleEditConfirm();
                    } else if (e.key === 'Escape') {
                      handleEditCancel();
                    }
                  }}
                  className="w-full px-3 py-2 border border-[var(--border-strong)] rounded-md focus:outline-none focus:ring-2 ring-[var(--gold-ring)] focus:border-transparent"
                  placeholder={t("Enter meeting title")}
                  autoFocus
                />
              </div>
              {sourceTitle?.meetingId === editModalState.meetingId
                && sourceTitle.title !== editModalState.currentTitle && (
                <div className="rounded-md border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-3">
                  <p className="text-xs font-medium text-[var(--fg3)]">{t('Original recording title')}</p>
                  <p className="mt-1 break-words text-sm text-[var(--fg1)]">{sourceTitle.title}</p>
                  <button
                    type="button"
                    onClick={() => setEditingTitle(sourceTitle.title)}
                    className="mt-2 text-xs font-medium text-[var(--gold)] hover:underline"
                  >
                    {t('Use original title')}
                  </button>
                </div>
              )}
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={handleEditCancel}
              className="px-4 py-2 text-sm font-medium text-[var(--fg2)] bg-[var(--bg-elevated)] hover:brightness-125 rounded-md transition-colors"
            >
              {t('Cancel')}
            </button>
            <button
              onClick={handleEditConfirm}
              className="px-4 py-2 text-sm font-medium text-[var(--fg-inverse)] bg-[var(--gold)] hover:bg-[var(--gold-active)] rounded-md transition-colors"
            >
              {t('Save')}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default Sidebar;
