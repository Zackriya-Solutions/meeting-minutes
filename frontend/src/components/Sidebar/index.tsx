'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import { File, Settings, ChevronLeftCircle, ChevronRightCircle, Home, Trash2, Mic, Square, Plus, Pencil, NotebookPen, SearchIcon, X, Upload } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
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
import { MeetingSections } from './MeetingSections';
import { TagFilterChips } from './TagFilterChips';
import { MeetingTagChips } from './MeetingTagChips';
import { TagManager } from '@/components/organization/TagManager';
import { NewFolderButton } from './NewFolderButton';
import { MeetingItemMenu } from './MeetingItemMenu';
import { MeetingOrganizationBar } from '@/components/organization/MeetingOrganizationBar';
import { buildSidebarSections, type Folder, type MeetingListItem } from '@/lib/organization';
import { organizationService } from '@/services/organizationService';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const {
    currentMeeting,
    setCurrentMeeting,
    isCollapsed,
    toggleCollapse,
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    setMeetings,
    serverAddress,
    folders,
    tags,
    trashItems,
    selectedTagIds,
    toggleTagFilter,
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

  // State for edit modal
  const [editModalState, setEditModalState] = useState<{ isOpen: boolean; meetingId: string | null; currentTitle: string }>({
    isOpen: false,
    meetingId: null,
    currentTitle: ''
  });
  const [editingTitle, setEditingTitle] = useState<string>('');

  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000);
  //   }
  // }, [settingsSaveSuccess]);


  const [tagsDialogMeetingId, setTagsDialogMeetingId] = useState<string | null>(null);
  const [folderRename, setFolderRename] = useState<{ folder: Folder; name: string } | null>(null);
  const [newFolderFor, setNewFolderFor] = useState<{ meetingId: string; name: string } | null>(null);
  const [folderDelete, setFolderDelete] = useState<Folder | null>(null);
  const [manageTagsOpen, setManageTagsOpen] = useState(false);

  const sections = useMemo(() => buildSidebarSections(meetings, folders, {
    selectedTagIds,
    searchQuery,
    searchResultIds: searchResults.map((r: { id: string }) => r.id),
  }), [meetings, folders, selectedTagIds, searchQuery, searchResults]);

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

  const handleTrash = async (itemId: string) => {
    try {
      await organizationService.trashMeeting(itemId);
      setMeetings(meetings.filter(m => m.id !== itemId));
      toast.success('Moved to Trash', {
        action: {
          label: 'Undo',
          onClick: async () => {
            try {
              await organizationService.restoreMeeting(itemId);
            } catch (error) {
              toast.error(String(error));
            }
          },
        },
      });

      // If trashing the active meeting, navigate to home
      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
        router.push('/');
      }
    } catch (error) {
      console.error('Failed to move meeting to Trash:', error);
      toast.error('Failed to move meeting to Trash', { description: String(error) });
    }
  };

  const moveMeeting = async (meetingId: string, folderId: string | null) => {
    try {
      await organizationService.setMeetingFolder(meetingId, folderId);
    } catch (error) {
      toast.error(String(error));
    }
  };

  // Creates a folder from a meeting's "Move to folder" menu and files the meeting in it.
  const createFolderForMeeting = async () => {
    if (!newFolderFor || !newFolderFor.name.trim()) return;
    try {
      const folder = await organizationService.createFolder(newFolderFor.name);
      await organizationService.setMeetingFolder(newFolderFor.meetingId, folder.id);
      setNewFolderFor(null);
    } catch (error) {
      toast.error(String(error));
    }
  };

  // Handle modal editing of meeting names
  const handleEditStart = (meetingId: string, currentTitle: string) => {
    setEditModalState({
      isOpen: true,
      meetingId: meetingId,
      currentTitle: currentTitle
    });
    setEditingTitle(currentTitle);
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    const meetingId = editModalState.meetingId;

    if (!meetingId) return;

    // Prevent empty titles
    if (!newTitle) {
      toast.error("Meeting title cannot be empty");
      return;
    }

    try {
      await invoke('api_save_meeting_title', {
        meetingId: meetingId,
        title: newTitle,
      });

      // Update local state
      const updatedMeetings = meetings.map(m =>
        m.id === meetingId ? { ...m, title: newTitle } : m
      );
      setMeetings(updatedMeetings);

      // Update current meeting if it's the one being edited
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }

      // Track the edit
      Analytics.trackButtonClick('edit_meeting_title', 'sidebar');

      toast.success("Meeting title updated successfully");

      // Close modal and reset state
      setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
      setEditingTitle('');
    } catch (error) {
      console.error('Failed to update meeting title:', error);
      toast.error("Failed to update meeting title", {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleEditCancel = () => {
    setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
    setEditingTitle('');
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

    return (
      <TooltipProvider>
        <div className="flex flex-col items-center space-y-4 mt-4">
          <Logo isCollapsed={isCollapsed} />

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isHomePage ? 'bg-gray-100' : 'hover:bg-gray-100'
                  }`}
              >
                <Home className="w-5 h-5 text-gray-600" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Home</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={handleRecordingToggle}
                disabled={isRecording}
                className={`p-2 ${isRecording ? 'bg-red-500 cursor-not-allowed' : 'bg-red-500 hover:bg-red-600'} rounded-full transition-colors duration-150 shadow-sm`}
              >
                {isRecording ? (
                  <Square className="w-5 h-5 text-white" />
                ) : (
                  <Mic className="w-5 h-5 text-white" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{isRecording ? "Recording in progress..." : "Start Recording"}</p>
            </TooltipContent>
          </Tooltip>

          {betaFeatures.importAndRetranscribe && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => openImportDialog()}
                  className="p-2 rounded-lg transition-colors duration-150 hover:bg-blue-100 bg-blue-50"
                >
                  <Upload className="w-5 h-5 text-blue-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Import Audio</p>
              </TooltipContent>
            </Tooltip>
          )}

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={toggleCollapse}
                className={`p-2 rounded-lg transition-colors duration-150 ${isMeetingPage ? 'bg-gray-100' : 'hover:bg-gray-100'
                  }`}
              >
                <NotebookPen className="w-5 h-5 text-gray-600" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Meeting Notes</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/settings')}
                className={`p-2 rounded-lg transition-colors duration-150 ${isSettingsPage ? 'bg-gray-100' : 'hover:bg-gray-100'
                  }`}
              >
                <Settings className="w-5 h-5 text-gray-600" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Settings</p>
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

  const renderMeetingRow = (meeting: MeetingListItem) => {
    const isActive = currentMeeting?.id === meeting.id;
    const isMeetingItem = meeting.id.includes('-') && !meeting.id.startsWith('intro-call');

    // Check if this item has a matching transcript snippet
    const matchingResult = isMeetingItem ? findMatchingSnippet(meeting.id) : null;
    const hasTranscriptMatch = !!matchingResult;

    return (
      <div key={meeting.id}>
        <div
          className={`flex items-center transition-all duration-150 group px-3 py-2 my-0.5 rounded-md text-sm ${isActive ? 'bg-blue-100 text-blue-700 font-medium' :
            hasTranscriptMatch ? 'bg-yellow-50' : 'hover:bg-gray-50'
            } cursor-pointer`}
          style={{ paddingLeft: '24px' }}
          onClick={() => {
            setCurrentMeeting({ id: meeting.id, title: meeting.title });
            const basePath = meeting.id.startsWith('intro-call') ? '/' :
              meeting.id.includes('-') ? `/meeting-details?id=${meeting.id}` : `/notes/${meeting.id}`;
            router.push(basePath);
          }}
        >
          <div className="flex flex-col w-full">
            <div className="flex items-center w-full">
              {isMeetingItem ? (
                <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-gray-100">
                  <File className="w-3.5 h-3.5 text-gray-600" />
                </div>
              ) : (
                <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-blue-100">
                  <Plus className="w-3.5 h-3.5 text-blue-600" />
                </div>
              )}
              <span className="flex-1 break-words">{meeting.title}</span>
              {isMeetingItem && (
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleEditStart(meeting.id, meeting.title);
                    }}
                    className="hover:text-blue-600 p-1 rounded-md hover:bg-blue-50 flex-shrink-0"
                    aria-label="Edit meeting title"
                  >
                    <Pencil className="w-4 h-4" />
                  </button>
                  <MeetingItemMenu
                    folders={folders}
                    folderId={meeting.folder_id}
                    onMove={folderId => void moveMeeting(meeting.id, folderId)}
                    onNewFolder={() => setNewFolderFor({ meetingId: meeting.id, name: '' })}
                    onEditTags={() => setTagsDialogMeetingId(meeting.id)}
                    onTrash={() => void handleTrash(meeting.id)}
                  />
                </div>
              )}
            </div>

            <MeetingTagChips tagIds={meeting.tag_ids} tags={tags} selectedIds={selectedTagIds} onToggle={toggleTagFilter} />

            {/* Show transcript match snippet if available */}
            {hasTranscriptMatch && (
              <div className="mt-1 ml-8 text-xs text-gray-500 bg-yellow-50 p-1.5 rounded border border-yellow-100 line-clamp-2">
                <span className="font-medium text-yellow-600">Match:</span> {matchingResult.matchContext}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      {/* Floating collapse button */}
      <button
        onClick={toggleCollapse}
        className="absolute -right-6 top-20 z-50 p-1 bg-white hover:bg-gray-100 rounded-full shadow-lg border"
        style={{ transform: 'translateX(50%)' }}
      >
        {isCollapsed ? (
          <ChevronRightCircle className="w-6 h-6" />
        ) : (
          <ChevronLeftCircle className="w-6 h-6" />
        )}
      </button>

      <div
        className={`h-screen bg-white border-r shadow-sm flex flex-col transition-all duration-300 ${isCollapsed ? 'w-16' : 'w-64'
          }`}
      >
        {/*  Header with traffic light spacing */}
        <div className="flex-shrink-0 h-22 flex items-center">

          {/* Title container */}



          <div className="flex-1">
            {!isCollapsed && (
              <div className="p-3">
                {/* <span className="text-lg text-center border rounded-full bg-blue-50 border-white font-semibold text-gray-700 mb-2 block items-center">
                  <span>Meetily</span>
                </span> */}
                <Logo isCollapsed={isCollapsed} />

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
                <TagFilterChips tags={tags} selectedIds={selectedTagIds} onToggle={toggleTagFilter} onManage={() => setManageTagsOpen(true)} />
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
                className="p-3  text-lg font-semibold items-center hover:bg-gray-100 h-10   flex mx-3 mt-3 rounded-lg cursor-pointer"
              >
                <Home className="w-4 h-4 mr-2" />
                <span>Home</span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0">
            {renderCollapsedIcons()}
            {!isCollapsed && (
              <div className="flex-1 overflow-y-auto custom-scrollbar min-h-0 pt-2">
                <NewFolderButton />
                {searchQuery && isSearching && (
                  <div className="mx-3 mb-1 text-xs text-blue-500 animate-pulse">Searching...</div>
                )}
                <MeetingSections
                  sections={sections}
                  renderMeeting={renderMeetingRow}
                  onDropMeeting={(meetingId, folderId) => void moveMeeting(meetingId, folderId)}
                  onRenameFolder={folder => setFolderRename({ folder, name: folder.name })}
                  onDeleteFolder={folder => setFolderDelete(folder)}
                />
              </div>
            )}
            {!isCollapsed && (
              <button
                type="button"
                onClick={() => router.push('/trash')}
                className="mx-3 mb-2 flex items-center gap-2 rounded-md px-3 py-2 text-sm text-gray-600 hover:bg-gray-50"
              >
                <Trash2 className="h-4 w-4" /> Trash{trashItems.length ? ` (${trashItems.length})` : ''}
              </button>
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
              v0.4.1
            </div>
          </div>
        )}
      </div>

      <Dialog open={tagsDialogMeetingId !== null} onOpenChange={open => !open && setTagsDialogMeetingId(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogTitle>Folder &amp; tags</DialogTitle>
          {tagsDialogMeetingId && <MeetingOrganizationBar meetingId={tagsDialogMeetingId} />}
        </DialogContent>
      </Dialog>

      <Dialog open={manageTagsOpen} onOpenChange={setManageTagsOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogTitle>Manage tags</DialogTitle>
          <TagManager />
        </DialogContent>
      </Dialog>

      <Dialog open={newFolderFor !== null} onOpenChange={open => !open && setNewFolderFor(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogTitle>New folder</DialogTitle>
          <input
            autoFocus
            placeholder="Folder name"
            value={newFolderFor?.name ?? ''}
            onChange={e => newFolderFor && setNewFolderFor({ ...newFolderFor, name: e.target.value })}
            onKeyDown={e => {
              if (e.key === 'Enter') void createFolderForMeeting();
            }}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          />
          <p className="text-xs text-gray-500">Press Enter to create it. The meeting moves into the new folder.</p>
        </DialogContent>
      </Dialog>

      <Dialog open={folderRename !== null} onOpenChange={open => !open && setFolderRename(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogTitle>Rename folder</DialogTitle>
          <input
            autoFocus
            value={folderRename?.name ?? ''}
            onChange={e => folderRename && setFolderRename({ ...folderRename, name: e.target.value })}
            onKeyDown={async e => {
              if (e.key !== 'Enter' || !folderRename) return;
              try {
                await organizationService.renameFolder(folderRename.folder.id, folderRename.name);
                setFolderRename(null);
              } catch (error) {
                toast.error(String(error));
              }
            }}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          />
        </DialogContent>
      </Dialog>

      <ConfirmationModal
        isOpen={folderDelete !== null}
        text={folderDelete
          ? `Delete folder "${folderDelete.name}"? ${meetings.filter(m => m.folder_id === folderDelete.id).length} meeting(s) will move to Unfiled.`
          : ''}
        onConfirm={async () => {
          if (!folderDelete) return;
          try {
            await organizationService.deleteFolder(folderDelete.id);
          } catch (error) {
            toast.error(String(error));
          }
          setFolderDelete(null);
        }}
        onCancel={() => setFolderDelete(null)}
      />

      {/* Edit Meeting Title Modal */}
      <Dialog open={editModalState.isOpen} onOpenChange={(open) => {
        if (!open) handleEditCancel();
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>Edit Meeting Title</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">Edit Meeting Title</h3>
            <div className="space-y-4">
              <div>
                <label htmlFor="meeting-title" className="block text-sm font-medium text-gray-700 mb-2">
                  Meeting Title
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
                  className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                  placeholder="Enter meeting title"
                  autoFocus
                />
              </div>
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={handleEditCancel}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-md transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleEditConfirm}
              className="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
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
