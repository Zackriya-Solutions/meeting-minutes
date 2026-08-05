'use client';

import type { Meta, StoryObj } from '@storybook/nextjs';
import { cn } from '@/lib/utils';

type UpcomingMeeting = {
  id: string;
  day: string;
  month: string;
  weekday: string;
  title: string;
  time: string;
};

const meetings: UpcomingMeeting[] = [
  {
    id: 'topic-explainer',
    day: '31',
    month: 'Июля',
    weekday: 'Пятница',
    title: 'Нерелевантный вызов topic explainer',
    time: '16:00',
  },
  {
    id: 'ai-daily-aug-3',
    day: '3',
    month: 'Августа',
    weekday: 'Понедельник',
    title: 'Ежедневная летучка ИИ‑помощника',
    time: '09:30',
  },
  {
    id: 'hypotheses',
    day: '3',
    month: 'Августа',
    weekday: 'Понедельник',
    title: 'Работа с гипотезами',
    time: '13:00',
  },
  {
    id: 'management-webinar',
    day: '3',
    month: 'Августа',
    weekday: 'Понедельник',
    title: 'Управленческий апгрейд_Вебинар',
    time: '14:30',
  },
  {
    id: 'ai-daily-aug-4',
    day: '4',
    month: 'Августа',
    weekday: 'Вторник',
    title: 'Ежедневная летучка ИИ‑помощника',
    time: '09:30',
  },
];

function UpcomingMeetingsDemo({ theme }: { theme: 'light' | 'dark' }) {
  return (
    <main
      className={cn(
        theme,
        'min-h-screen bg-[var(--elevation-1)] text-foreground',
      )}
    >
      <section className="home-screen home-screen--static">
        <div className="home-screen__inner !w-full !max-w-[920px]">
          <header className="home-screen__header">
            <h1 className="memento-screen-title">Предстоящие встречи</h1>
          </header>

          <div className="home-meeting-card">
            {meetings.map((meeting) => (
              <button
                key={meeting.id}
                type="button"
                className="home-meeting-row no-drag"
                aria-label={`Открыть встречу: ${meeting.title}`}
              >
                <span className="home-meeting-date">
                  <span className="home-meeting-day home-display">{meeting.day}</span>
                  <span className="home-meeting-date-copy">
                    <span className="home-meeting-month">{meeting.month}</span>
                    <span className="home-meeting-weekday">{meeting.weekday}</span>
                  </span>
                </span>

                <span className="home-meeting-main">
                  <span className="home-meeting-accent" aria-hidden="true" />
                  <span className="home-meeting-copy">
                    <span className="home-meeting-title">{meeting.title}</span>
                    <span className="home-meeting-time">{meeting.time}</span>
                  </span>
                </span>
              </button>
            ))}
          </div>
        </div>
      </section>
    </main>
  );
}

const meta = {
  title: 'Встречи/Предстоящие встречи',
  component: UpcomingMeetingsDemo,
  parameters: {
    layout: 'fullscreen',
  },
} satisfies Meta<typeof UpcomingMeetingsDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Dark: Story = {
  name: 'Тёмная тема',
  args: { theme: 'dark' },
};

export const Light: Story = {
  name: 'Светлая тема',
  args: { theme: 'light' },
};
