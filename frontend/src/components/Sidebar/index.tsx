'use client';

import React, { useState, useMemo, useEffect, useCallback, useRef } from 'react';
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
import { invoke } from '@tauri-apps/api/core';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';
import { cn } from '@/lib/utils';

import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';

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

/**
 * Every control in the rail, at both widths. One primitive rather than an
 * icon version and a label version — the collapsed and expanded rails used to
 * be separate JSX trees, which is how they drifted apart.
 *
 * `iconOnly` is the width, not the rail state: the collapse toggle and the
 * footer utilities stay icons even in the expanded rail. Being on a route
 * reads as a filled row — the brand-soft language. An open *meeting* is
 * content and gets an edge rule instead, so the two never look like the same
 * kind of thing. See /DESIGN.md § Component rules.
 *
 * `tone` covers the capture affordances: `danger` is the idle record button,
 * `live` is that same slot once capture is running.
 */
function RailRow({
  icon: Icon,
  label,
  active,
  tone,
  iconOnly,
  quiet,
  onClick,
  children,
}: {
  icon?: typeof Home;
  label: string;
  active?: boolean;
  tone?: 'danger' | 'live';
  iconOnly?: boolean;
  quiet?: boolean;
  onClick: () => void;
  children?: React.ReactNode;
}) {
  const button = (
    <button
      onClick={onClick}
      aria-label={iconOnly ? label : undefined}
      aria-current={active ? 'page' : undefined}
      className={cn(
        'flex h-8 items-center rounded-md transition-colors duration-fast',
        iconOnly
          ? 'w-8 shrink-0 justify-center'
          : 'w-full gap-2 px-gutter text-sm font-medium',
        tone === 'danger'
          ? 'bg-danger text-white hover:bg-danger-hover'
          : tone === 'live'
            ? 'bg-danger-soft text-danger-ink'
            : active
              ? 'bg-brand-soft text-brand-soft-ink'
              : cn(
                  quiet ? 'text-ink-faint' : 'text-ink-muted',
                  'hover:bg-ink/5 hover:text-ink active:bg-ink/10'
                )
      )}
    >
      {children ?? (Icon ? <Icon className="h-4 w-4 shrink-0" aria-hidden /> : null)}
      {!iconOnly && label}
    </button>
  );

  // A visible label needs no tooltip repeating it.
  if (!iconOnly) return button;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
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

  const searchRef = useRef<HTMLInputElement>(null);
  const [wantSearchFocus, setWantSearchFocus] = useState(false);

  const isHome = pathname === '/';
  const isSettings = pathname === '/settings';

  // The Rust tray opens settings through this. Kept as-is.
  useEffect(() => {
    (window as any).openSettings = () => router.push('/settings');
    return () => {
      delete (window as any).openSettings;
    };
  }, [router]);

  // The collapsed rail's search icon expands the rail. It also has to land the
  // cursor in the field — otherwise the control promises search and delivers a
  // panel. See /DESIGN.md § Component rules.
  useEffect(() => {
    if (isCollapsed || !wantSearchFocus) return;
    searchRef.current?.focus();
    setWantSearchFocus(false);
  }, [isCollapsed, wantSearchFocus]);

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

  const openSettings = () => router.push('/settings');
  const openSession = () => router.push('/');

  return (
    <>
      {/* Five zones, ranked: identity · capture · find · views + meetings ·
          utilities. One component drives both widths — the collapsed and
          expanded rails used to be separate returns, which is how they drifted
          apart. Every zone insets by --rail-gutter, so every row's content box
          starts on the same vertical line. */}
      <aside
        aria-label="Sidebar"
        className="fixed left-0 top-0 z-rail flex h-screen flex-col border-r border-line bg-panel"
        style={{ width: isCollapsed ? 'var(--rail-w-collapsed)' : 'var(--rail-w)' }}
      >
        {/* 1 · Identity. The mark's junction dot is the live indicator. */}
        <div
          className={cn(
            'flex gap-1',
            isCollapsed
              ? 'flex-col items-center pt-2'
              : 'items-center px-gutter pb-1 pt-2'
          )}
        >
          <Logo isCollapsed={isCollapsed} live={isRecording} />
          <RailRow
            iconOnly
            quiet
            icon={isCollapsed ? PanelLeftOpen : PanelLeftClose}
            label={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            onClick={toggleCollapse}
          />
          {isCollapsed && <div className="my-1 h-px w-6 bg-line" />}
        </div>

        {/* 2 · Capture. The rail's first job is getting audio in, so it sits
            above anything that scrolls — never under a list 100 rows long.
            On Home the in-page transport already owns the control, so this
            slot reports state there instead of shipping a second red button.
            It keeps its height either way: changing route must not shuffle
            the rail underneath the pointer. */}
        <div
          className={cn(
            isCollapsed ? 'flex flex-col items-center gap-1' : 'px-gutter pb-1'
          )}
        >
          {isRecording ? (
            isCollapsed ? (
              <RailRow
                iconOnly
                tone="live"
                label="Recording — open session"
                onClick={openSession}
              >
                <span className="h-2 w-2 rounded-full bg-danger animate-live" />
              </RailRow>
            ) : isHome ? (
              <div className="flex h-9 items-center rounded-md border border-danger/30 bg-danger-soft px-gutter">
                <LiveIndicator />
              </div>
            ) : (
              <button
                onClick={openSession}
                className="flex h-9 w-full items-center justify-between rounded-md border border-danger/30 bg-danger-soft px-gutter transition-colors duration-fast hover:border-danger/50"
              >
                <LiveIndicator />
                <span className="text-2xs text-ink-muted">Open</span>
              </button>
            )
          ) : isHome ? (
            /* Idle on Home: the in-page transport owns the control, so the
               rail reports capture state rather than duplicating it. */
            isCollapsed ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span
                    role="status"
                    aria-label="Not recording"
                    className="flex h-8 w-8 items-center justify-center text-ink-faint"
                  >
                    <span
                      aria-hidden
                      className="h-2 w-2 rounded-full border border-current"
                    />
                  </span>
                </TooltipTrigger>
                <TooltipContent side="right">Not recording</TooltipContent>
              </Tooltip>
            ) : (
              <div
                role="status"
                className="flex h-9 items-center gap-2 px-gutter text-ink-faint"
              >
                <span
                  aria-hidden
                  className="h-2 w-2 shrink-0 rounded-full border border-current"
                />
                <span className="text-xs">Not recording</span>
              </div>
            )
          ) : isCollapsed ? (
            <RailRow
              iconOnly
              tone="danger"
              icon={Mic}
              label="Start recording"
              onClick={handleRecordingToggle}
            />
          ) : (
            <Button
              onClick={handleRecordingToggle}
              variant="destructive"
              className="h-9 w-full gap-2"
            >
              <Mic className="h-4 w-4" aria-hidden />
              Start recording
            </Button>
          )}

          {betaFeatures.importAndRetranscribe &&
            (isCollapsed ? (
              <RailRow
                iconOnly
                icon={Upload}
                label="Import audio"
                onClick={() => openImportDialog()}
              />
            ) : (
              <Button
                onClick={() => openImportDialog()}
                variant="outline"
                className="mt-1.5 h-8 w-full gap-2 text-sm"
              >
                <Upload className="h-3.5 w-3.5" aria-hidden />
                Import audio
              </Button>
            ))}
        </div>

        {/* 3 · Find */}
        <div
          className={cn(
            isCollapsed
              ? 'flex flex-col items-center gap-1 pt-1'
              : 'px-gutter pb-2 pt-1'
          )}
        >
          {isCollapsed ? (
            <RailRow
              iconOnly
              icon={Search}
              label="Search meetings"
              onClick={() => {
                setWantSearchFocus(true);
                toggleCollapse();
              }}
            />
          ) : (
            <div className="relative">
              <Search
                aria-hidden
                className="pointer-events-none absolute left-gutter top-1/2 h-4 w-4 -translate-y-1/2 text-ink-faint"
              />
              <input
                ref={searchRef}
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
          )}
        </div>

        {/* 4 · Views and meetings. Home heads the group; the meetings are its
            content, so they share one left axis and one scroll region. */}
        <nav
          aria-label="Views and meetings"
          className="flex min-h-0 flex-1 flex-col"
        >
          <div
            className={cn(isCollapsed ? 'flex justify-center' : 'px-gutter pb-1')}
          >
            <RailRow
              icon={Home}
              label="Home"
              active={isHome}
              iconOnly={isCollapsed}
              onClick={openSession}
            />
          </div>

          {!isCollapsed && (
            <div className="scrollbar-slim min-h-0 flex-1 overflow-y-auto px-gutter pb-2">
              {/* Sticky, so it still says what the list is at row 40. */}
              <div className="sticky top-0 z-sticky flex items-baseline justify-between bg-panel px-gutter pb-1.5 pt-2">
                <h2 className="text-2xs font-semibold uppercase tracking-wider text-ink-faint">
                  Meetings
                </h2>
                {meetingItems.length > 0 && (
                  <span className="readout text-2xs text-ink-faint">
                    {meetingItems.length}
                  </span>
                )}
              </div>

              {meetingItems.length === 0 ? (
                <p className="px-gutter py-3 text-xs leading-relaxed text-ink-muted">
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
                            'group relative flex items-start gap-1 rounded-md pl-gutter pr-1',
                            'transition-colors duration-fast',
                            active
                              ? 'text-ink'
                              : 'text-ink-muted hover:bg-ink/5 hover:text-ink'
                          )}
                        >
                          {/* Document selection: a brand edge in the gutter,
                              not a fill. A route you are on is chrome; an
                              open meeting is content. */}
                          {active && (
                            <span
                              aria-hidden
                              className="absolute bottom-1 left-0 top-1 w-0.5 rounded-full bg-brand"
                            />
                          )}

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
                                  setEditModalState({
                                    isOpen: true,
                                    meetingId: item.id,
                                  });
                                  setEditingTitle(item.title);
                                }}
                                aria-label={`Rename ${item.title}`}
                                className="flex h-6 w-6 items-center justify-center rounded-sm text-ink-faint transition-colors duration-fast hover:bg-ink/10 hover:text-ink"
                              >
                                <Pencil className="h-3.5 w-3.5" />
                              </button>
                              <button
                                onClick={() =>
                                  setDeleteModalState({
                                    isOpen: true,
                                    itemId: item.id,
                                  })
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
          )}
        </nav>

        {/* 5 · Utilities. Settings is a route and keeps the route shape; the
            theme cycle and About are controls, and the version is a readout.
            Three different kinds of thing, three different weights. */}
        <div
          className={cn(
            isCollapsed
              ? 'mt-auto flex flex-col items-center gap-1 pb-2'
              : 'border-t border-line px-gutter py-2'
          )}
        >
          <RailRow
            icon={Settings}
            label="Settings"
            active={isSettings}
            iconOnly={isCollapsed}
            onClick={openSettings}
          />
          {isCollapsed ? (
            <>
              <ThemeToggleButton />
              <Info isCollapsed />
            </>
          ) : (
            <div className="mt-0.5 flex items-center gap-0.5">
              <ThemeToggleButton />
              <Info isCollapsed={false} />
              <span className="readout ml-auto pr-1 text-2xs text-ink-faint">
                v{APP_VERSION}
              </span>
            </div>
          )}
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
