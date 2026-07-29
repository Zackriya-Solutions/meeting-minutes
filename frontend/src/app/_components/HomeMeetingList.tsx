'use client';

import { useMemo } from 'react';
import { useRouter } from 'next/navigation';
import { Icon } from '@/components/memento/Icon';
import { Button } from '@/components/ui/button';
import { useSidebar, type CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { useLanguage } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';

type MeetingGroup = {
  key: 'today' | 'yesterday' | 'earlier';
  label: string;
  meetings: CurrentMeeting[];
};

function meetingTimestamp(meeting: CurrentMeeting): number {
  const value = meeting.occurredAt ?? meeting.createdAt;
  if (!value) return 0;
  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function meetingBucket(meeting: CurrentMeeting): MeetingGroup['key'] {
  const timestamp = meetingTimestamp(meeting);
  if (!timestamp) return 'earlier';

  const date = new Date(timestamp);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  if (date.getTime() >= today) return 'today';
  if (date.getTime() >= yesterday.getTime()) return 'yesterday';
  return 'earlier';
}

export function HomeMeetingList() {
  const router = useRouter();
  const { meetings, setCurrentMeeting } = useSidebar();
  const { t, lang } = useLanguage();

  const groups = useMemo<MeetingGroup[]>(() => {
    const result: MeetingGroup[] = [
      { key: 'today', label: t('Today'), meetings: [] },
      { key: 'yesterday', label: t('Yesterday'), meetings: [] },
      { key: 'earlier', label: t('Earlier'), meetings: [] },
    ];
    const byKey = new Map(result.map((group) => [group.key, group]));

    [...meetings]
      .sort((left, right) => meetingTimestamp(right) - meetingTimestamp(left))
      .forEach((meeting) => byKey.get(meetingBucket(meeting))?.meetings.push(meeting));

    return result.filter((group) => group.meetings.length > 0);
  }, [meetings, t]);

  const openMeeting = (meeting: CurrentMeeting) => {
    setCurrentMeeting(meeting);
    router.push(`/meeting-details?id=${meeting.id}`);
  };

  if (groups.length === 0) return null;

  return (
    <div className="flex min-h-[calc(100vh-8rem)] w-full items-center justify-center px-8 py-10">
      <div className="max-h-[calc(100vh-10rem)] w-full max-w-[560px] overflow-y-auto custom-scrollbar">
        <div className="space-y-10">
          {groups.map((group) => (
            <section key={group.key} className={group.key === 'earlier' ? 'opacity-70' : ''}>
              <h2 className="mb-3 px-3 text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                {group.label}
              </h2>
              <div className="space-y-1">
                {group.meetings.map((meeting) => {
                  const display = getMeetingDisplayInfo(meeting, lang);
                  return (
                    <Button
                      key={meeting.id}
                      type="button"
                      variant="ghost"
                      onClick={() => openMeeting(meeting)}
                      className="group h-auto w-full justify-start gap-4 rounded-xl px-4 py-3 text-left"
                    >
                      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
                        <Icon name="transcript" size={18} />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate text-base font-medium text-foreground">
                          {display.title}
                        </span>
                        <span className={`mt-0.5 block text-sm ${display.dateUnknown ? 'text-primary' : 'text-muted-foreground'}`}>
                          {display.dateLabel}
                        </span>
                      </span>
                    </Button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
