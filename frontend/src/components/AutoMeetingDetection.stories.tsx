'use client';

import type { Meta, StoryObj } from '@storybook/nextjs';
import { MeetingDetectionBanner } from './MeetingDetectionBanner';

const meta = {
  title: 'Система/Определение встречи',
  parameters: {
    layout: 'fullscreen',
  },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

function ReferenceStage({ children }: { children: React.ReactNode }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      {children}
    </main>
  );
}

function GoogleMeetMark() {
  return (
    <span aria-hidden="true" className="relative block h-10 w-12 shrink-0">
      <span className="absolute inset-y-1 left-0 w-8 rounded-[10px] bg-[#ffcc19]" />
      <span className="absolute right-0 top-[9px] h-[22px] w-[16px] rounded-[3px] bg-[#ff9f0a] [clip-path:polygon(0_28%,100%_0,100%_100%,0_72%)]" />
      <span className="absolute bottom-[7px] left-[6px] h-[7px] w-[7px] rounded-full bg-white" />
    </span>
  );
}

function GranolaPromptCard() {
  return (
    <section
      aria-label="Референс Granola: предстоящая встреча"
      className="flex min-h-[112px] w-full max-w-[760px] items-stretch overflow-hidden rounded-[24px] border border-black/10 bg-[#fafaf7] text-[#282827] shadow-[0_16px_36px_rgba(0,0,0,0.14)]"
    >
      <div className="flex min-w-0 flex-1 items-center gap-6 px-6 py-4">
        <span className="h-14 w-[6px] shrink-0 rounded-full bg-[#ff5d4e]" />
        <div className="min-w-0">
          <h2 className="truncate text-[25px] font-medium leading-[1.2] tracking-[-0.025em]">
            Bizzy.fm weekly sync
          </h2>
          <p className="mt-1 text-[22px] leading-none text-[#727270]">18:10–18:40</p>
        </div>
      </div>

      <div className="my-4 w-px bg-black/10" />

      <div className="flex shrink-0 items-stretch">
        <button
          type="button"
          className="flex items-center gap-3 px-5 text-left transition-colors hover:bg-black/[0.035]"
        >
          <GoogleMeetMark />
          <span>
            <span className="block whitespace-nowrap text-[23px] font-medium leading-tight tracking-[-0.015em]">
              Join Google Meet
            </span>
            <span className="block whitespace-nowrap text-[20px] leading-tight text-[#777775]">
              &amp; open Granola
            </span>
          </span>
        </button>
        <button
          type="button"
          aria-label="Другие действия"
          className="flex w-14 items-center justify-center border-l border-black/10 text-[26px] text-[#363634] transition-colors hover:bg-black/[0.035]"
        >
          ⌄
        </button>
      </div>
    </section>
  );
}

function MementoPromptDemo() {
  return (
    <ReferenceStage>
      <p className="text-sm text-muted-foreground">Плашка появляется поверх приложения при старте встречи.</p>
      <MeetingDetectionBanner
        open
        state="recording"
        appNames={['Zoom']}
        onPrimaryAction={() => undefined}
        onDismiss={() => undefined}
      />
    </ReferenceStage>
  );
}

export const GranolaReference: Story = {
  name: 'Granola — референс',
  render: () => (
    <ReferenceStage>
      <GranolaPromptCard />
    </ReferenceStage>
  ),
};

export const MementoCurrent: Story = {
  name: 'Memento — текущая карточка',
  render: () => <MementoPromptDemo />,
};
