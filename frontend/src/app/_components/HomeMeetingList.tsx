'use client';

import {
  type FormEvent,
  type MouseEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { Icon } from '@/components/memento/Icon';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { PromptInput } from '@/components/ui/prompt-input';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { useSidebar, type CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { RecordingStatus, useRecordingState } from '@/contexts/RecordingStateContext';
import { useLanguage } from '@/lib/i18n';
import Analytics from '@/lib/analytics';
import { clearMarkedMoments } from '@/lib/markedMoments';
import { formatRelativeMeetingDate, getMeetingDisplayInfo } from '@/lib/meetingDisplay';
import { prefetchMeetingSummary } from '@/lib/meetingSummaryCache';
import { isRecordingNavigationLocked } from '@/lib/recordingNavigation';
import { splitSummaryLead, summaryToMarkdown } from '@/lib/summaryToMarkdown';
import { Cell, CellText } from '@/vendor/deslop/mini-app/Cell';
import { IconPlus } from '@/vendor/deslop/primitives/material-symbols-react';
import { buildArchivePromptSuggestions } from '@/lib/promptSuggestions';
import { spring } from '@/lib/fluid/springs';
import { CalendarSettings } from '@/components/CalendarSettings';
import type { LocalOutlookMeeting } from '@/lib/localOutlookCalendar';

interface CalendarMeeting {
  meeting: CurrentMeeting;
  title: string;
  date: Date | null;
  timeRange: string;
  description: string | null;
}

interface CalendarMeetingGroup {
  key: string;
  label: string;
  meetings: CalendarMeeting[];
}

interface FluidMeetingRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

function FluidMeetingList({
  children,
  itemCount,
}: {
  children: ReactNode;
  itemCount: number;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const animationFrameRef = useRef<number | null>(null);
  const sessionRef = useRef(0);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [itemRects, setItemRects] = useState<FluidMeetingRect[]>([]);

  const measureItems = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const containerRect = container.getBoundingClientRect();
    const nextRects = Array.from(
      container.querySelectorAll<HTMLElement>('[data-fluid-meeting-index]')
    ).map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        top: rect.top - containerRect.top - container.clientTop,
        left: rect.left - containerRect.left - container.clientLeft,
        width: rect.width,
        height: rect.height,
      };
    });
    setItemRects(nextRects);
  }, []);

  useLayoutEffect(() => {
    measureItems();
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver(measureItems);
    observer.observe(container);
    return () => observer.disconnect();
  }, [itemCount, measureItems]);

  useEffect(() => () => {
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
    }
  }, []);

  const handleMouseMove = (event: MouseEvent<HTMLDivElement>) => {
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
    }

    const mouseY = event.clientY;
    animationFrameRef.current = requestAnimationFrame(() => {
      animationFrameRef.current = null;
      const container = containerRef.current;
      if (!container) return;

      const containerTop = container.getBoundingClientRect().top;
      let closestIndex: number | null = null;
      let closestDistance = Number.POSITIVE_INFINITY;

      itemRects.forEach((rect, index) => {
        const distance = Math.abs(mouseY - (containerTop + rect.top + rect.height / 2));
        if (distance < closestDistance) {
          closestDistance = distance;
          closestIndex = index;
        }
      });
      setActiveIndex(closestIndex);
    });
  };

  const activeRect = activeIndex === null ? null : itemRects[activeIndex];

  return (
    <div
      ref={containerRef}
      className="home-history__list no-drag"
      onMouseEnter={() => {
        sessionRef.current += 1;
      }}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setActiveIndex(null)}
    >
      <AnimatePresence>
        {activeRect && (
          <motion.span
            key={sessionRef.current}
            aria-hidden="true"
            className="home-history__hover-indicator"
            initial={{ ...activeRect, opacity: 0 }}
            animate={{ ...activeRect, opacity: 1 }}
            exit={{ opacity: 0, transition: spring.fast.exit }}
            transition={{ ...spring.fast, opacity: { duration: 0.08 } }}
          />
        )}
      </AnimatePresence>
      {children}
    </div>
  );
}

export const HOME_SCROLL_POSITION_KEY = 'memento:home-scroll-position';

function rememberHomeScrollPosition(target: HTMLElement) {
  const scrollContainer = target.closest<HTMLElement>('[data-home-scroll-container]');
  if (!scrollContainer) return;

  window.sessionStorage.setItem(HOME_SCROLL_POSITION_KEY, String(scrollContainer.scrollTop));
}

function meetingDate(meeting: CurrentMeeting): Date | null {
  const value = meeting.occurredAt ?? meeting.createdAt;
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function meetingTimestamp(meeting: CurrentMeeting): number {
  return meetingDate(meeting)?.getTime() ?? 0;
}

function meetingDurationMinutes(meeting: CurrentMeeting): number | null {
  const seconds = meeting.durationSeconds;
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds <= 0) return null;
  return Math.max(1, Math.round(seconds / 60));
}

function historyDateKey(date: Date | null): string {
  if (!date) return 'unknown';

  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, '0'),
    String(date.getDate()).padStart(2, '0'),
  ].join('-');
}

const MEETING_DESCRIPTION_MAX_WORDS = 4;
const MEETING_DESCRIPTION_MAX_CHARACTERS = 56;

function plainSummaryLine(value: string): string {
  return value
    .replace(/<!--[^]*?-->/g, ' ')
    .replace(/```[^]*?```/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/^\s*(?:#{1,6}|[-+*]|\d+[.)])\s+/gm, '')
    .replace(/[*_~`>|]/g, ' ')
    .replace(/\s+/g, ' ')
    .replace(/^(?:эта\s+)?(?:встреча|созвон|совещание)\s+(?:состоял[ао]?|включал[ао]?).*?[.!?]\s*/iu, '')
    .replace(/^(?:эта\s+)?(?:встреча|созвон|совещание)\s+(?:был[ао]?\s+)?посвящ(?:ена|ено|ён|ен)\s+/iu, '')
    .replace(/^(?:эта\s+)?(?:встреча|созвон|совещание)\s+представля(?:ет|л[ао]?)\s+собой\s+/iu, '')
    .replace(/^(?:эта\s+)?(?:встреча|встречи|встречу|созвон|совещание)(?=[\s,:—–-]|$)[\s,:—–-]*/iu, '')
    .replace(/^(?:в\s+первой\s+части|сначала)\s+/iu, '')
    .replace(/^обсуждению(?=\s|$)/iu, 'Обсуждение')
    .replace(/^координации(?=\s|$)/iu, 'Координация')
    .replace(/^организации(?=\s|$)/iu, 'Организация')
    .replace(/^планированию(?=\s|$)/iu, 'Планирование')
    .replace(/^разбору(?=\s|$)/iu, 'Разбор')
    .trim();
}

function truncateMeetingDescription(value: string): string | null {
  const plainText = plainSummaryLine(value);
  if (!plainText) return null;
  const text = `${plainText.charAt(0).toLocaleUpperCase()}${plainText.slice(1)}`;

  const words = text.split(/\s+/);
  const selected: string[] = [];

  for (const word of words) {
    const next = [...selected, word].join(' ');
    if (
      selected.length >= MEETING_DESCRIPTION_MAX_WORDS
      || (selected.length > 0 && Array.from(next).length > MEETING_DESCRIPTION_MAX_CHARACTERS)
    ) {
      break;
    }
    selected.push(word);
  }

  if (selected.length === 0) return null;

  return selected.join(' ').replace(/[.,:;!?—–-]+$/u, '');
}

function firstSummaryTopic(markdown: string): string | null {
  const topicHeading = /^#{1,6}\s+(?:основные темы(?: обсуждения)?|темы обсуждения|main topics(?: discussed)?|topics discussed)\s*$/imu;
  const headingMatch = topicHeading.exec(markdown);
  if (!headingMatch) return null;

  const sectionStart = headingMatch.index + headingMatch[0].length;
  const remainingMarkdown = markdown.slice(sectionStart);
  const nextHeadingIndex = remainingMarkdown.search(/^#{1,6}\s+/m);
  const topicSection = nextHeadingIndex >= 0
    ? remainingMarkdown.slice(0, nextHeadingIndex)
    : remainingMarkdown;
  const firstTopicLine = topicSection
    .split('\n')
    .map((line) => line.trim())
    .find((line) => /^[-+*]\s+/.test(line));

  if (!firstTopicLine) return null;

  const boldTopic = firstTopicLine.match(/^[-+*]\s+\*\*(.+?)\*\*/)?.[1];
  return truncateMeetingDescription(boldTopic ?? firstTopicLine);
}

function meetingSummaryDescription(summary: unknown): string | null {
  const markdown = summaryToMarkdown(summary);
  if (!markdown) return null;

  const topic = firstSummaryTopic(markdown);
  if (topic) return topic;

  const { lead } = splitSummaryLead(markdown);
  if (lead) return truncateMeetingDescription(lead);

  const firstContentLine = markdown
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line && !/^#{1,6}\s/.test(line) && !/^\|?\s*:?-{3,}/.test(line));

  return firstContentLine ? truncateMeetingDescription(firstContentLine) : null;
}

export function HomeMeetingList({ animateOnMount = true }: { animateOnMount?: boolean }) {
  const router = useRouter();
  const { isRecording, status } = useRecordingState();
  const {
    currentMeeting,
    meetings,
    refetchMeetings,
    setCurrentMeeting,
    stopSummaryPolling,
  } = useSidebar();
  const { t, lang } = useLanguage();
  const [question, setQuestion] = useState('');
  const [meetingToDelete, setMeetingToDelete] = useState<CalendarMeeting | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [meetingDescriptions, setMeetingDescriptions] = useState<Record<string, string>>({});
  const screenRef = useRef<HTMLDivElement>(null);
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  const navigationLocked = isRecordingNavigationLocked(isRecording, status);
  const canStartMeeting = !navigationLocked && (
    status === RecordingStatus.IDLE
    || status === RecordingStatus.COMPLETED
    || status === RecordingStatus.ERROR
  );

  const startNewMeeting = () => {
    if (!canStartMeeting) return;

    window.sessionStorage.setItem('autoStartRecording', 'true');
    router.push('/recording');
  };

  // The provider owns the archive request and retries when the WebView is not
  // ready yet. Only request on an empty initial state; route drawers render
  // this view in the background and mount the home view again on close, so an
  // unconditional request here would cause a second render during hand-off.
  useEffect(() => {
    if (meetings.length === 0) void refetchMeetings();
  }, [meetings.length, refetchMeetings]);

  useLayoutEffect(() => {
    const scrollContainer = screenRef.current?.closest<HTMLElement>('[data-home-scroll-container]');
    const storedPosition = Number(window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY));

    if (scrollContainer && Number.isFinite(storedPosition)) {
      scrollContainer.scrollTop = Math.max(0, storedPosition);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    const loadDescriptions = async () => {
      const entries = await Promise.all(
        meetings.map(async (meeting) => {
          const summary = await prefetchMeetingSummary(meeting.id);
          return [meeting.id, meetingSummaryDescription(summary)] as const;
        }),
      );

      if (cancelled) return;

      setMeetingDescriptions(Object.fromEntries(
        entries.filter((entry): entry is readonly [string, string] => entry[1] !== null),
      ));
    };

    void loadDescriptions();
    return () => {
      cancelled = true;
    };
  }, [meetings]);

  const calendarMeetings = useMemo<CalendarMeeting[]>(() => {
    const timeFormatter = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });

    return [...meetings]
      .sort((left, right) => meetingTimestamp(right) - meetingTimestamp(left))
      .map((meeting) => {
        const display = getMeetingDisplayInfo(meeting, lang);
        const date = meetingDate(meeting);
        const durationSeconds = meeting.durationSeconds;
        const durationMinutes = meetingDurationMinutes(meeting);
        const end = date && durationMinutes && typeof durationSeconds === 'number'
          ? new Date(date.getTime() + durationSeconds * 1_000)
          : null;
        return {
          meeting,
          title: display.title,
          date,
          timeRange: date
            ? end
              ? `${timeFormatter.format(date)}\u00a0–\u00a0${timeFormatter.format(end)}`
              : timeFormatter.format(date)
            : display.dateLabel,
          description: meetingDescriptions[meeting.id] ?? null,
        };
      });
  }, [lang, locale, meetingDescriptions, meetings]);

  const calendarMeetingGroups = useMemo<CalendarMeetingGroup[]>(() => {
    const groups = new Map<string, CalendarMeetingGroup>();

    calendarMeetings.forEach((meeting) => {
      const key = historyDateKey(meeting.date);
      const existingGroup = groups.get(key);

      if (existingGroup) {
        existingGroup.meetings.push(meeting);
        return;
      }

      groups.set(key, {
        key,
        label: formatRelativeMeetingDate(meeting.date, locale, t),
        meetings: [meeting],
      });
    });

    return Array.from(groups.values());
  }, [calendarMeetings, locale, t]);

  const archiveSuggestions = useMemo(
    () => buildArchivePromptSuggestions(
      calendarMeetings.map((item) => ({
        title: item.title,
        description: item.description,
      })),
      lang,
    ),
    [calendarMeetings, lang],
  );
  const openMeeting = (meeting: CurrentMeeting, event: MouseEvent<HTMLButtonElement>) => {
    rememberHomeScrollPosition(event.currentTarget);
    setCurrentMeeting(meeting);
    router.push(`/meeting-details?id=${encodeURIComponent(meeting.id)}`);
  };

  const openUpcomingMeeting = (meeting: LocalOutlookMeeting, event: MouseEvent<HTMLButtonElement>) => {
    rememberHomeScrollPosition(event.currentTarget);
    const title = meeting.subject.trim() || t('Upcoming meeting');
    setCurrentMeeting({
      id: meeting.id,
      title,
      occurredAt: meeting.start_at,
    });
    const params = new URLSearchParams({
      id: meeting.id,
      upcoming: '1',
      title,
      start: meeting.start_at,
      end: meeting.end_at,
    });
    router.push(`/meeting-details?${params.toString()}`);
  };

  const askArchive = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const query = question.trim();
    router.push(query ? `/chat?question=${encodeURIComponent(query)}` : '/chat');
  };

  const deleteMeeting = async () => {
    const item = meetingToDelete;
    if (!item || isDeleting) return;

    setIsDeleting(true);
    try {
      await invoke('api_delete_meeting', {
        meetingId: item.meeting.id,
        deleteRecordingFiles: false,
      });
      stopSummaryPolling(item.meeting.id);
      clearMarkedMoments(item.meeting.id);
      await refetchMeetings();
      Analytics.trackMeetingDeleted(item.meeting.id);

      if (currentMeeting?.id === item.meeting.id) {
        setCurrentMeeting({ id: 'intro-call', title: t('+ New Call') });
      }
      setMeetingToDelete(null);
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      toast.error(t('Failed to delete meeting'), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div ref={screenRef} className={`home-screen min-h-full${animateOnMount ? '' : ' home-screen--static'}`}>
      <div className="home-screen__inner">
        <header className="home-screen__header">
          <h1 className="memento-screen-title">{t('Meetings')}</h1>
          <FluidButton
            type="button"
            variant="tertiary"
            size="icon-lg"
            className="no-drag rounded-[20px] text-[var(--primary-50)] [&>span:last-child]:absolute [&>span:last-child]:inset-0 [&_.deslop-material-symbol]:flex [&_.deslop-material-symbol]:h-5 [&_.deslop-material-symbol]:w-5 [&_.deslop-material-symbol]:items-center [&_.deslop-material-symbol]:justify-center"
            onClick={startNewMeeting}
            disabled={!canStartMeeting}
            aria-label={t('New meeting')}
            title={t('New meeting')}
          >
            <IconPlus aria-hidden="true" size={20} weight={400} />
          </FluidButton>
        </header>

        <main className="home-screen__content">
          <CalendarSettings variant="home" onOpenMeeting={openUpcomingMeeting} />

          {calendarMeetings.length > 0 ? (
            <section className="home-history" aria-label={t('Earlier')}>
              <div className="home-history__groups">
                {calendarMeetingGroups.map((group) => (
                  <section key={group.key} className="home-history__group" aria-label={group.label}>
                    <h3>{group.label}</h3>
                    <FluidMeetingList itemCount={group.meetings.length}>
                      {group.meetings.map((item, itemIndex) => (
                        <ContextMenu key={item.meeting.id}>
                          <ContextMenuTrigger asChild>
                            <div className="contents">
                              <Cell
                                type="button"
                                className="home-history-cell no-drag"
                                data-fluid-meeting-index={itemIndex}
                                data-separator={itemIndex < group.meetings.length - 1}
                                onClick={(event) => openMeeting(item.meeting, event)}
                                end={<span className="home-history-time">{item.timeRange}</span>}
                              >
                                <CellText title={item.title} description={item.description} bold />
                              </Cell>
                            </div>
                          </ContextMenuTrigger>
                          <ContextMenuContent className="w-[210px] rounded-[14px] p-1.5">
                            <ContextMenuItem
                              onSelect={() => setMeetingToDelete(item)}
                              className="gap-2 rounded-[9px] px-2.5 py-[9px] text-destructive focus:bg-destructive/10 focus:text-destructive"
                            >
                              <Icon name="trash" size={16} />
                              <span>{t('Delete meeting')}</span>
                            </ContextMenuItem>
                          </ContextMenuContent>
                        </ContextMenu>
                      ))}
                    </FluidMeetingList>
                  </section>
                ))}
              </div>
            </section>
          ) : null}
        </main>
      </div>

      <form className="no-drag prompt-input-fade home-ask-dock" onSubmit={askArchive}>
        <PromptInput
          value={question}
          onValueChange={setQuestion}
          submitButtonType="submit"
          containerClassName="home-ask"
          placeholder={t('Ask anything')}
          placeholderSuggestions={archiveSuggestions}
          aria-label={t('Ask anything')}
          sendLabel={t('Send')}
        />
      </form>

      <AlertDialog
        open={meetingToDelete !== null}
        onOpenChange={(open) => {
          if (!open && !isDeleting) setMeetingToDelete(null);
        }}
      >
        <AlertDialogContent className="max-w-md">
          <AlertDialogHeader>
            <AlertDialogTitle>{t('Delete this meeting?')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('Are you sure you want to delete this meeting? This action cannot be undone.')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isDeleting}>{t('Cancel')}</AlertDialogCancel>
            <AlertDialogAction
              disabled={isDeleting}
              onClick={() => void deleteMeeting()}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isDeleting ? t('Deleting...') : t('Delete meeting')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
