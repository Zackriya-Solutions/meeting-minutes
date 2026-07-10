'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import { File, Settings, Home, Trash2, Mic, Square, Pencil, NotebookPen, SearchIcon, X, Upload, Folder as FolderIcon, FolderPlus, PanelLeft } from 'lucide-react';
import { useRouter, usePathname, useSearchParams } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
import { ConfirmationModal } from '../ConfirmationModel/confirmation-modal';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '../SettingTabs';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';

import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"

import { MessageToast } from '../MessageToast';
import Info from '../Info';
import { ComplianceNotification } from '../ComplianceNotification';
import { Input } from '../ui/input';
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from '../ui/input-group';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const {
    setCurrentMeeting,
    isCollapsed,
    toggleCollapse,
    sidebarWidth,
    setSidebarWidth,
    isResizingSidebar,
    setIsResizingSidebar,
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    folders,
    createFolder,
    renameFolder,
    deleteFolder,
    serverAddress
  } = useSidebar();

  // Get recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();
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

  // Folder nav state (Granola-style sidebar)
  const [activeFolderId, setActiveFolderId] = useState<string | null>(null);
  const [isUncategorizedActive, setIsUncategorizedActive] = useState(false);
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [folderRenameState, setFolderRenameState] = useState<{ isOpen: boolean; folderId: string | null }>({ isOpen: false, folderId: null });
  const [renamingFolderName, setRenamingFolderName] = useState('');
  const [folderDeleteState, setFolderDeleteState] = useState<{ isOpen: boolean; folderId: string | null }>({ isOpen: false, folderId: null });

  // Keep the highlighted folder or virtual Uncategorized view in sync with the URL.
  useEffect(() => {
    if (pathname === '/notes') {
      setActiveFolderId(searchParams.get('folder'));
      setIsUncategorizedActive(searchParams.get('view') === 'uncategorized');
    } else {
      setActiveFolderId(null);
      setIsUncategorizedActive(false);
    }
  }, [pathname, searchParams]);

  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000);
  //   }
  // }, [settingsSaveSuccess]);



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
  const handleSearchChange = useCallback(async (value: string) => {
    setSearchQuery(value);

    // If search query is empty, just return to normal view
    if (!value.trim()) return;

    // Search through transcripts
    await searchTranscripts(value);
  }, [searchTranscripts]);

  // Meetings matching the search, by transcript hit or title, newest first
  const searchMatches = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();
    const transcriptMatches = new Map(searchResults.map(result => [result.id, result]));

    return meetings
      .filter(m => transcriptMatches.has(m.id) || m.title.toLowerCase().includes(q))
      .map(m => ({ ...m, match: transcriptMatches.get(m.id) }))
      .sort((a, b) => new Date(b.created_at ?? 0).getTime() - new Date(a.created_at ?? 0).getTime());
  }, [searchQuery, searchResults, meetings]);

  // Meetings without a folder stay easy to find without being mixed into the folder list.
  const uncategorizedMeetings = useMemo(() => {
    return meetings
      .filter(meeting => meeting.folder_id === null || meeting.folder_id === undefined)
      .sort((a, b) => new Date(b.created_at ?? 0).getTime() - new Date(a.created_at ?? 0).getTime());
  }, [meetings]);


  // ----- Folder navigation & CRUD -----

  const openAllNotes = () => {
    setActiveFolderId(null);
    setIsUncategorizedActive(false);
    router.push('/notes');
  };

  const openUncategorized = () => {
    setActiveFolderId(null);
    setIsUncategorizedActive(true);
    router.push('/notes?view=uncategorized');
  };

  const openFolder = (folderId: string) => {
    setActiveFolderId(folderId);
    setIsUncategorizedActive(false);
    router.push(`/notes?folder=${folderId}`);
  };

  const handleCreateFolder = async () => {
    const name = newFolderName.trim();
    setIsCreatingFolder(false);
    setNewFolderName('');
    if (!name) return;

    const folder = await createFolder(name);
    if (folder) {
      Analytics.trackButtonClick('create_folder', 'sidebar');
      openFolder(folder.id);
    } else {
      toast.error('Failed to create folder');
    }
  };

  const handleFolderRenameConfirm = async () => {
    const name = renamingFolderName.trim();
    const folderId = folderRenameState.folderId;
    if (!folderId) return;

    if (!name) {
      toast.error('Folder name cannot be empty');
      return;
    }

    const ok = await renameFolder(folderId, name);
    if (ok) {
      toast.success('Folder renamed');
    } else {
      toast.error('Failed to rename folder');
    }
    setFolderRenameState({ isOpen: false, folderId: null });
    setRenamingFolderName('');
  };

  const handleFolderDeleteConfirm = async () => {
    const folderId = folderDeleteState.folderId;
    setFolderDeleteState({ isOpen: false, folderId: null });
    if (!folderId) return;

    const ok = await deleteFolder(folderId);
    if (ok) {
      toast.success('Folder deleted', { description: 'Its meetings were moved to All Notes' });
      if (activeFolderId === folderId) {
        openAllNotes();
      }
    } else {
      toast.error('Failed to delete folder');
    }
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




  // Drag-to-resize from the sidebar's right edge (expanded mode only)
  const startSidebarResize = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizingSidebar(true);

    const onMove = (ev: MouseEvent) => setSidebarWidth(ev.clientX);
    const onUp = () => {
      setIsResizingSidebar(false);
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  };

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      {/* Sidebar toggle - fixed at the window's top-left, Granola-style.
          Same position whether the sidebar is open or fully hidden. */}
      <button
        onClick={toggleCollapse}
        className="fixed top-2 left-2 z-50 p-1.5 rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-100 transition-colors"
        aria-label={isCollapsed ? 'Show sidebar' : 'Hide sidebar'}
        title={isCollapsed ? 'Show sidebar' : 'Hide sidebar'}
      >
        <PanelLeft className="w-[18px] h-[18px]" />
      </button>

      <div
        className={`h-screen bg-sidebar shadow-sm flex flex-col relative overflow-hidden ${isCollapsed ? '' : 'border-r'} ${isResizingSidebar ? '' : 'transition-all duration-300'}`}
        style={{ width: isCollapsed ? 0 : sidebarWidth }}
      >
        {/* Resize handle */}
        {!isCollapsed && (
          <div
            onMouseDown={startSidebarResize}
            className="absolute top-0 right-0 h-full w-1.5 cursor-col-resize z-50 hover:bg-blue-500/40 active:bg-blue-500/60 transition-colors"
            aria-hidden="true"
          />
        )}

        {/* Clearance for the fixed toggle button */}
        <div className="flex-shrink-0 h-11" />

        <div className="flex-shrink-0">
          <div className="flex-1">
            {!isCollapsed && (
              <div className="px-3 pb-1">
                <div className="relative mb-1">
                  <InputGroup >
                    <InputGroupInput placeholder='Search meeting content...' value={searchQuery}
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
                className="px-3 text-sm font-medium text-gray-700 items-center hover:bg-gray-100 h-9 flex mx-3 mt-2 rounded-lg cursor-pointer"
              >
                <Home className="w-4 h-4 mr-2" />
                <span>Home</span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0">
            {/* All Notes + Folders navigation (Granola-style), or search results */}
            {!isCollapsed && (
              <div className="flex-1 overflow-y-auto custom-scrollbar min-h-0 pb-2">
                {searchQuery.trim() ? (
                  <div className="mx-3 mt-3">
                    <div className="px-3 pb-1 text-xs font-semibold uppercase tracking-wider text-gray-400 flex items-center">
                      Results
                      {isSearching && <span className="ml-2 text-blue-500 animate-pulse normal-case font-normal">Searching...</span>}
                    </div>
                    {searchMatches.length === 0 && !isSearching && (
                      <div className="px-3 py-2 text-sm text-gray-500">No matches</div>
                    )}
                    {searchMatches.map(meeting => (
                      <div
                        key={meeting.id}
                        onClick={() => {
                          setCurrentMeeting({ id: meeting.id, title: meeting.title });
                          router.push(`/meeting-details?id=${meeting.id}`);
                        }}
                        className="px-3 py-2 my-0.5 rounded-md text-sm hover:bg-gray-100 cursor-pointer"
                      >
                        <div className="flex items-center">
                          <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-gray-100">
                            <File className="w-3.5 h-3.5 text-gray-600" />
                          </div>
                          <span className="flex-1 min-w-0 truncate">{meeting.title}</span>
                        </div>
                        {meeting.match && (
                          <div className="mt-1 ml-8 text-xs text-gray-500 bg-yellow-50 p-1.5 rounded border border-yellow-200 line-clamp-2">
                            <span className="font-medium text-yellow-600">Match:</span> {meeting.match.matchContext}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                ) : (
                  <>
                    {/* All Notes */}
                    <div
                      onClick={openAllNotes}
                      className={`px-3 text-sm font-medium text-gray-700 items-center h-9 flex mx-3 mt-1 rounded-lg cursor-pointer ${pathname === '/notes' && !activeFolderId && !isUncategorizedActive ? 'bg-gray-100' : 'hover:bg-gray-100'}`}
                    >
                      <NotebookPen className="w-4 h-4 mr-2" />
                      <span>All Notes</span>
                    </div>

                    {/* Virtual folder for meetings that have not been assigned to a folder */}
                    <div className="mx-3 mt-1">
                      <div
                        onClick={openUncategorized}
                        className={`px-3 py-2 my-0.5 rounded-md text-sm flex items-center group cursor-pointer ${isUncategorizedActive ? 'bg-blue-100 text-blue-700 font-medium' : 'text-gray-700 hover:bg-gray-100'}`}
                      >
                        <FolderIcon className="w-4 h-4 mr-2 flex-shrink-0" />
                        <span className="flex-1 min-w-0 truncate">Uncategorized</span>
                        <span className="ml-2 text-xs text-gray-400">{uncategorizedMeetings.length}</span>
                      </div>
                    </div>

                    {/* Folders */}
                    <div className="mx-3 mt-4 px-3 flex items-center justify-between text-xs font-semibold uppercase tracking-wider text-gray-400">
                      <span>Folders</span>
                      <button
                        onClick={() => setIsCreatingFolder(true)}
                        className="p-1 -mr-1 rounded hover:bg-gray-100 hover:text-gray-600 transition-colors"
                        aria-label="New folder"
                      >
                        <FolderPlus className="w-4 h-4" />
                      </button>
                    </div>

                    <div className="mx-3 mt-1">
                      {folders.map(folder => (
                        <div
                          key={folder.id}
                          onClick={() => openFolder(folder.id)}
                          className={`px-3 py-2 my-0.5 rounded-md text-sm flex items-center group cursor-pointer ${activeFolderId === folder.id ? 'bg-blue-100 text-blue-700 font-medium' : 'text-gray-700 hover:bg-gray-100'}`}
                        >
                          <FolderIcon className="w-4 h-4 mr-2 flex-shrink-0" />
                          <span className="flex-1 min-w-0 truncate">{folder.name}</span>
                          <span className="ml-2 text-xs text-gray-400 group-hover:hidden">{folder.meeting_count}</span>
                          <div className="hidden group-hover:flex items-center gap-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setFolderRenameState({ isOpen: true, folderId: folder.id });
                                setRenamingFolderName(folder.name);
                              }}
                              className="hover:text-blue-600 p-1 rounded-md hover:bg-blue-50 flex-shrink-0"
                              aria-label="Rename folder"
                            >
                              <Pencil className="w-3.5 h-3.5" />
                            </button>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setFolderDeleteState({ isOpen: true, folderId: folder.id });
                              }}
                              className="hover:text-red-600 p-1 rounded-md hover:bg-red-50 flex-shrink-0"
                              aria-label="Delete folder"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>
                      ))}

                      {isCreatingFolder && (
                        <div className="px-3 py-2 my-0.5 rounded-md text-sm flex items-center bg-gray-100">
                          <FolderIcon className="w-4 h-4 mr-2 flex-shrink-0 text-gray-500" />
                          <input
                            autoFocus
                            value={newFolderName}
                            onChange={(e) => setNewFolderName(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') handleCreateFolder();
                              if (e.key === 'Escape') { setIsCreatingFolder(false); setNewFolderName(''); }
                            }}
                            onBlur={handleCreateFolder}
                            placeholder="Folder name"
                            className="flex-1 min-w-0 bg-transparent outline-none text-sm placeholder:text-gray-500"
                          />
                        </div>
                      )}

                      {folders.length === 0 && !isCreatingFolder && (
                        <div className="px-3 py-2 text-xs text-gray-500">
                          No folders yet — create one to organize your meetings.
                        </div>
                      )}
                    </div>
                  </>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        {!isCollapsed && (

          <div className="flex-shrink-0 p-2 border-t border-gray-100">
            <button
              onClick={handleRecordingToggle}
              disabled={isRecording}
              className={`w-full flex items-center justify-center px-3 py-2 text-sm font-medium text-white ${isRecording ? 'bg-red-300 cursor-not-allowed' : 'bg-red-500 hover:bg-red-600'} rounded-lg transition-colors shadow-sm`}
            >
              {isRecording ? (
                <>
                  <Square className="w-4 h-4 mr-2" />
                  <span>Recording in progress...</span>
                </>
              ) : (
                <>
                  <Mic className="w-4 h-4 mr-2" />
                  <span>Start Recording</span>
                </>
              )}
            </button>

            {betaFeatures.importAndRetranscribe && (
              <button
                onClick={() => openImportDialog()}
                className="w-full flex items-center justify-center px-3 py-2 mt-1 text-sm font-medium text-gray-700 bg-blue-100 hover:bg-blue-200 rounded-lg transition-colors shadow-sm"
              >
                <Upload className="w-4 h-4 mr-2" />
                <span>Import Audio</span>
              </button>
            )}

            <button
              onClick={() => router.push('/settings')}
              className="w-full flex items-center justify-center px-3 py-1.5 mt-1 mb-1 text-sm font-medium text-gray-700 bg-gray-200 hover:bg-gray-300 rounded-lg transition-colors shadow-sm"
            >
              <Settings className="w-4 h-4 mr-2" />
              <span>Settings</span>
            </button>
            <Info isCollapsed={isCollapsed} />
            <div className="w-full flex items-center justify-center px-3 py-1 text-xs text-gray-400">
              v0.4.0
            </div>
          </div>
        )}
      </div>

      {/* Confirmation Modal for Folder Delete */}
      <ConfirmationModal
        isOpen={folderDeleteState.isOpen}
        text="Delete this folder? Its meetings will not be deleted — they'll move back to All Notes."
        onConfirm={handleFolderDeleteConfirm}
        onCancel={() => setFolderDeleteState({ isOpen: false, folderId: null })}
      />

      {/* Rename Folder Modal */}
      <Dialog open={folderRenameState.isOpen} onOpenChange={(open) => {
        if (!open) {
          setFolderRenameState({ isOpen: false, folderId: null });
          setRenamingFolderName('');
        }
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>Rename Folder</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">Rename Folder</h3>
            <div className="space-y-4">
              <div>
                <label htmlFor="folder-name" className="block text-sm font-medium text-gray-700 mb-2">
                  Folder Name
                </label>
                <input
                  id="folder-name"
                  type="text"
                  value={renamingFolderName}
                  onChange={(e) => setRenamingFolderName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      handleFolderRenameConfirm();
                    } else if (e.key === 'Escape') {
                      setFolderRenameState({ isOpen: false, folderId: null });
                      setRenamingFolderName('');
                    }
                  }}
                  className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                  placeholder="Enter folder name"
                  autoFocus
                />
              </div>
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={() => {
                setFolderRenameState({ isOpen: false, folderId: null });
                setRenamingFolderName('');
              }}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-md transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleFolderRenameConfirm}
              className="px-4 py-2 text-sm font-medium text-white bg-blue-700 hover:bg-blue-600 rounded-md transition-colors"
            >
              Save
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default Sidebar;
