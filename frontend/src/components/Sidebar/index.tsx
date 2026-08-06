'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import {
  Settings,
  PanelLeftClose,
  PanelLeftOpen,
  Home,
  Trash2,
  Mic,
  Search,
  Pencil,
  X,
  Upload,
  Loader2,
} from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { ConfirmationModal } from '../ConfirmationModel/confirmation-modal';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';
import { cn } from '@/lib/utils';

import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';

import Logo from '../Logo';
import Info from '../Info';
import { ThemeToggleButton } from '../ThemeToggle';
import { LiveIndicator } from '../LiveIndicator';
import { Button } from '../ui/button';

interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

const APP_VERSION = '0.4.0';

/** Icon-only rail action. One shape for every collapsed-state control. */
function RailIcon({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={onClick}
          aria-label={label}
          aria-current={active ? 'page' : undefined}
          className={cn(
            'flex h-8 w-8 items-center justify-center rounded-md',
            'transition-colors duration-fast',
            active
              ? 'bg-brand-soft text-brand-soft-ink'
              : 'text-ink-muted hover:bg-ink/5 hover:text-ink active:bg-ink/10'
          )}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
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
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    setMeetings,
  } = useSidebar();

  const { isRecording } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();

  const [searchQuery, setSearchQuery] = useState('');
  const [deleteModalState, setDeleteModalState] = useState<{
    isOpen: boolean;
    itemId: string | null;
  }>({ isOpen: false, itemId: null });
  const [editModalState, setEditModalState] = useState<{
    isOpen: boolean;
    meetingId: string | null;
  }>({ isOpen: false, meetingId: null });
  const [editingTitle, setEditingTitle] = useState('');

  const isHome = pathname === '/';
  const isSettings = pathname === '/settings';

  // The Rust tray opens settings through this. Kept as-is.
  useEffect(() => {
    (window as any).openSettings = () => router.push('/settings');
    return () => {
      delete (window as any).openSettings;
    };
  }, [router]);

  const handleSearchChange = useCallback(
    async (value: string) => {
      setSearchQuery(value);
      if (!value.trim()) return;
      await searchTranscripts(value);
    },
    [searchTranscripts]
  );

  /** Flattened meeting list — the rail shows one list, not a folder tree. */
  const meetingItems = useMemo(() => {
    const all: SidebarItem[] = sidebarItems.flatMap((item) =>
      item.type === 'folder' ? (item.children ?? []) : [item]
    );

    if (!searchQuery.trim()) return all;

    const matchedIds = new Set(searchResults.map((r) => r.id));
    const q = searchQuery.toLowerCase();
    return all.filter(
      (item) => matchedIds.has(item.id) || item.title.toLowerCase().includes(q)
    );
  }, [sidebarItems, searchQuery, searchResults]);

  const snippetFor = (id: string) =>
    searchQuery.trim() ? searchResults.find((r) => r.id === id) : undefined;

  const handleDelete = async (itemId: string) => {
    try {
      await invoke('api_delete_meeting', { meetingId: itemId });
      setMeetings(meetings.filter((m: CurrentMeeting) => m.id !== itemId));
      Analytics.trackMeetingDeleted(itemId);
      toast.success('Meeting deleted', {
        description: 'The recording, transcript, and summary were removed.',
      });

      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
        router.push('/');
      }
    } catch (error) {
      toast.error('Could not delete meeting', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    const meetingId = editModalState.meetingId;
    if (!meetingId) return;

    if (!newTitle) {
      toast.error('Meeting title cannot be empty');
      return;
    }

    try {
      await invoke('api_save_meeting_title', { meetingId, title: newTitle });
      setMeetings(
        meetings.map((m: CurrentMeeting) =>
          m.id === meetingId ? { ...m, title: newTitle } : m
        )
      );
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }
      Analytics.trackButtonClick('edit_meeting_title', 'sidebar');
      toast.success('Meeting renamed');
      setEditModalState({ isOpen: false, meetingId: null });
      setEditingTitle('');
    } catch (error) {
      toast.error('Could not rename meeting', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const openMeeting = (item: SidebarItem) => {
    setCurrentMeeting({ id: item.id, title: item.title });
    const path = item.id.startsWith('intro-call')
      ? '/'
      : item.id.includes('-')
        ? `/meeting-details?id=${item.id}`
        : `/notes/${item.id}`;
    router.push(path);
  };

  /* ------------------------------------------------------------------ */
  /* Collapsed rail                                                      */
  /* ------------------------------------------------------------------ */

  if (isCollapsed) {
    return (
      <aside
        className="fixed left-0 top-0 z-rail flex h-screen flex-col items-center gap-1 border-r border-line bg-panel py-3"
        style={{ width: 'var(--rail-w-collapsed)' }}
      >
        <Logo isCollapsed live={isRecording} />

        <RailIcon label="Expand sidebar" onClick={toggleCollapse}>
          <PanelLeftOpen className="h-4 w-4" />
        </RailIcon>

        <div className="my-1 h-px w-6 bg-line" />

        <RailIcon label="Home" active={isHome} onClick={() => router.push('/')}>
          <Home className="h-4 w-4" />
        </RailIcon>

        {/* Same rule as the expanded rail: on Home the transport owns this. */}
        {(isRecording || !isHome) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={isRecording ? () => router.push('/') : handleRecordingToggle}
                aria-label={
                  isRecording ? 'Recording in progress — go to session' : 'Start recording'
                }
                className={cn(
                  'flex h-8 w-8 items-center justify-center rounded-md transition-colors duration-fast',
                  isRecording
                    ? 'bg-danger-soft text-danger-ink'
                    : 'bg-danger text-white hover:bg-danger-hover'
                )}
              >
                {isRecording ? (
                  <span className="h-2 w-2 rounded-full bg-danger animate-live" />
                ) : (
                  <Mic className="h-4 w-4" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              {isRecording ? 'Recording — open session' : 'Start recording'}
            </TooltipContent>
          </Tooltip>
        )}

        {betaFeatures.importAndRetranscribe && (
          <RailIcon label="Import audio" onClick={() => openImportDialog()}>
            <Upload className="h-4 w-4" />
          </RailIcon>
        )}

        <RailIcon label="Search meetings" onClick={toggleCollapse}>
          <Search className="h-4 w-4" />
        </RailIcon>

        <div className="mt-auto flex flex-col items-center gap-1">
          <ThemeToggleButton />
          <RailIcon
            label="Settings"
            active={isSettings}
            onClick={() => router.push('/settings')}
          >
            <Settings className="h-4 w-4" />
          </RailIcon>
          <Info isCollapsed />
        </div>
      </aside>
    );
  }

  /* ------------------------------------------------------------------ */
  /* Expanded rail                                                       */
  /* ------------------------------------------------------------------ */

  return (
    <>
      <aside
        className="fixed left-0 top-0 z-rail flex h-screen flex-col border-r border-line bg-panel"
        style={{ width: 'var(--rail-w)' }}
      >
        {/* Header */}
        <div className="flex items-center gap-1 px-2 pb-1 pt-3">
          <Logo isCollapsed={false} live={isRecording} />
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={toggleCollapse}
                aria-label="Collapse sidebar"
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-ink-faint transition-colors duration-fast hover:bg-ink/5 hover:text-ink"
              >
                <PanelLeftClose className="h-4 w-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Collapse sidebar</TooltipContent>
          </Tooltip>
        </div>

        {/* Search */}
        <div className="px-3 pb-2 pt-1">
          <div className="relative">
            <Search
              aria-hidden
              className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-ink-faint"
            />
            <input
              type="search"
              value={searchQuery}
              onChange={(e) => handleSearchChange(e.target.value)}
              placeholder="Search transcripts"
              aria-label="Search meeting transcripts"
              className={cn(
                'h-8 w-full rounded-md border border-line bg-sunken pl-8 pr-7 text-sm text-ink',
                'placeholder:text-ink-muted',
                'transition-colors duration-fast',
                'hover:border-line-strong focus:border-brand focus:bg-elevated',
                '[&::-webkit-search-cancel-button]:appearance-none'
              )}
            />
            {isSearching ? (
              <Loader2
                aria-hidden
                className="absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 animate-spin text-ink-faint"
              />
            ) : (
              searchQuery && (
                <button
                  onClick={() => handleSearchChange('')}
                  aria-label="Clear search"
                  className="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-sm text-ink-faint transition-colors duration-fast hover:bg-ink/5 hover:text-ink"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              )
            )}
          </div>
        </div>

        {/* Primary nav */}
        <nav className="px-3 pb-2">
          <button
            onClick={() => router.push('/')}
            aria-current={isHome ? 'page' : undefined}
            className={cn(
              'flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm font-medium',
              'transition-colors duration-fast',
              isHome
                ? 'bg-brand-soft text-brand-soft-ink'
                : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
            )}
          >
            <Home className="h-4 w-4 shrink-0" aria-hidden />
            Home
          </button>
        </nav>

        {/* Meetings */}
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex items-baseline justify-between px-5 pb-1.5 pt-2">
            <h2 className="text-2xs font-semibold uppercase tracking-wider text-ink-faint">
              Meetings
            </h2>
            {meetingItems.length > 0 && (
              <span className="readout text-2xs text-ink-faint">
                {meetingItems.length}
              </span>
            )}
          </div>

          <div className="scrollbar-slim min-h-0 flex-1 overflow-y-auto px-3 pb-2">
            {meetingItems.length === 0 ? (
              <p className="px-2 py-3 text-xs leading-relaxed text-ink-muted">
                {searchQuery
                  ? `Nothing matches “${searchQuery}”.`
                  : 'No meetings yet. Start a recording and it will appear here.'}
              </p>
            ) : (
              <ul className="space-y-px">
                {meetingItems.map((item) => {
                  const active = currentMeeting?.id === item.id;
                  const snippet = snippetFor(item.id);
                  const isNewCall = item.id.startsWith('intro-call');

                  return (
                    <li key={item.id}>
                      <div
                        className={cn(
                          'group relative flex items-start gap-1 rounded-md pl-2 pr-1',
                          'transition-colors duration-fast',
                          active
                            ? 'bg-brand-soft text-brand-soft-ink'
                            : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
                        )}
                      >
                        <button
                          onClick={() => openMeeting(item)}
                          aria-current={active ? 'page' : undefined}
                          className="min-w-0 flex-1 py-1.5 text-left text-sm"
                        >
                          <span
                            className={cn(
                              'block truncate',
                              active && 'font-medium',
                              isNewCall && 'text-brand-soft-ink'
                            )}
                          >
                            {item.title}
                          </span>
                          {snippet && (
                            <span className="mt-0.5 line-clamp-2 block text-2xs leading-snug text-ink-faint">
                              {snippet.matchContext}
                            </span>
                          )}
                        </button>

                        {!isNewCall && (
                          <div
                            className={cn(
                              'flex shrink-0 items-center gap-0.5 self-center',
                              'opacity-0 transition-opacity duration-fast',
                              'group-hover:opacity-100 group-focus-within:opacity-100'
                            )}
                          >
                            <button
                              onClick={() => {
                                setEditModalState({ isOpen: true, meetingId: item.id });
                                setEditingTitle(item.title);
                              }}
                              aria-label={`Rename ${item.title}`}
                              className="flex h-6 w-6 items-center justify-center rounded-sm text-ink-faint transition-colors duration-fast hover:bg-ink/10 hover:text-ink"
                            >
                              <Pencil className="h-3.5 w-3.5" />
                            </button>
                            <button
                              onClick={() =>
                                setDeleteModalState({ isOpen: true, itemId: item.id })
                              }
                              aria-label={`Delete ${item.title}`}
                              className="flex h-6 w-6 items-center justify-center rounded-sm text-ink-faint transition-colors duration-fast hover:bg-danger-soft hover:text-danger-ink"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        )}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </div>

        {/* Record. On Home the floating transport already owns this action, so
            the rail stays quiet there rather than showing a second red button. */}
        <div className="border-t border-line p-3">
          {isRecording ? (
            <button
              onClick={() => router.push('/')}
              className="flex h-9 w-full items-center justify-between rounded-md border border-danger/30 bg-danger-soft px-3 transition-colors duration-fast hover:border-danger/50"
            >
              <LiveIndicator />
              <span className="text-2xs text-ink-muted">Open</span>
            </button>
          ) : (
            !isHome && (
              <Button
                onClick={handleRecordingToggle}
                variant="destructive"
                className="h-9 w-full gap-2"
              >
                <Mic className="h-4 w-4" aria-hidden />
                Start recording
              </Button>
            )
          )}

          {betaFeatures.importAndRetranscribe && (
            <Button
              onClick={() => openImportDialog()}
              variant="outline"
              className={cn('h-8 w-full gap-2 text-sm', (isRecording || !isHome) && 'mt-1.5')}
            >
              <Upload className="h-3.5 w-3.5" aria-hidden />
              Import audio
            </Button>
          )}

          <div className="mt-2 flex items-center gap-0.5">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/settings')}
                  aria-label="Settings"
                  aria-current={isSettings ? 'page' : undefined}
                  className={cn(
                    'flex h-8 w-8 items-center justify-center rounded-md transition-colors duration-fast',
                    isSettings
                      ? 'bg-brand-soft text-brand-soft-ink'
                      : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
                  )}
                >
                  <Settings className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">Settings</TooltipContent>
            </Tooltip>

            <Info isCollapsed />
            <ThemeToggleButton />

            <span className="readout ml-auto pr-1 text-2xs text-ink-faint">
              v{APP_VERSION}
            </span>
          </div>
        </div>
      </aside>

      <ConfirmationModal
        isOpen={deleteModalState.isOpen}
        text="Delete this meeting? The recording, transcript, and summary are removed from this machine. This cannot be undone."
        onConfirm={() => {
          if (deleteModalState.itemId) handleDelete(deleteModalState.itemId);
          setDeleteModalState({ isOpen: false, itemId: null });
        }}
        onCancel={() => setDeleteModalState({ isOpen: false, itemId: null })}
      />

      <Dialog
        open={editModalState.isOpen}
        onOpenChange={(open) => {
          if (!open) {
            setEditModalState({ isOpen: false, meetingId: null });
            setEditingTitle('');
          }
        }}
      >
        <DialogContent className="sm:max-w-[420px]">
          <DialogTitle className="text-xl">Rename meeting</DialogTitle>
          <div className="pt-1">
            <label
              htmlFor="meeting-title"
              className="mb-1.5 block text-sm font-medium text-ink"
            >
              Title
            </label>
            <input
              id="meeting-title"
              type="text"
              value={editingTitle}
              onChange={(e) => setEditingTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleEditConfirm();
              }}
              className="h-9 w-full rounded-md border border-line-strong bg-canvas px-3 text-base text-ink transition-colors duration-fast focus:border-brand"
              placeholder="Weekly planning"
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setEditModalState({ isOpen: false, meetingId: null });
                setEditingTitle('');
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleEditConfirm}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
};

export default Sidebar;
