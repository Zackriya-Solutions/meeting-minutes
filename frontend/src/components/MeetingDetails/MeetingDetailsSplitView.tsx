'use client';

import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { FileText, Sparkles } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
const STORAGE_KEY = 'meetily.meetingDetails.transcriptPaneRatio';
const DEFAULT_RATIO = 0.3;
const MIN_RATIO = 0.3;
const MAX_RATIO = 0.5;

const TABS = [
  { value: 'transcript' as const, label: 'Transcript', icon: FileText },
  { value: 'summary' as const, label: 'Summary', icon: Sparkles },
];

function readStoredRatio(): number {
  if (typeof window === 'undefined') return DEFAULT_RATIO;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const n = raw == null ? NaN : Number(raw);
    return Number.isFinite(n) && n >= MIN_RATIO && n <= MAX_RATIO ? n : DEFAULT_RATIO;
  } catch {
    return DEFAULT_RATIO;
  }
}

function writeStoredRatio(value: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch {
    // Layout persistence is optional.
  }
}

export type MeetingDetailsTab = 'transcript' | 'summary';

interface MeetingDetailsSplitViewProps {
  transcript: ReactNode;
  summary: ReactNode;
  activeTab: MeetingDetailsTab;
  onTabChange: (tab: MeetingDetailsTab) => void;
}

export function MeetingDetailsSplitView({
  transcript,
  summary,
  activeTab,
  onTabChange,
}: MeetingDetailsSplitViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [ratio, setRatio] = useState(DEFAULT_RATIO);
  const [isDesktop, setIsDesktop] = useState(true);
  const dragging = useRef(false);
  const clampRatio = (value: number) => Math.min(MAX_RATIO, Math.max(MIN_RATIO, value));

  useEffect(() => {
    setRatio(readStoredRatio());
  }, []);

  useEffect(() => {
    const mediaQuery = window.matchMedia('(min-width: 768px)');
    const updateLayout = () => setIsDesktop(mediaQuery.matches);
    updateLayout();
    mediaQuery.addEventListener('change', updateLayout);
    return () => mediaQuery.removeEventListener('change', updateLayout);
  }, []);

  const onPointerDown = useCallback((event: React.PointerEvent) => {
    event.preventDefault();
    dragging.current = true;
    event.currentTarget.setPointerCapture(event.pointerId);
  }, []);

  const onPointerMove = useCallback((event: React.PointerEvent) => {
    if (!dragging.current || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    setRatio(clampRatio((event.clientX - rect.left) / rect.width));
  }, []);

  const onPointerUp = useCallback(() => {
    if (!dragging.current) return;
    dragging.current = false;
    setRatio((current) => {
      writeStoredRatio(current);
      return current;
    });
  }, []);

  const onSeparatorKeyDown = useCallback((event: React.KeyboardEvent) => {
    const current = ratio;
    const next =
      event.key === 'ArrowLeft' ? clampRatio(current - 0.05) :
      event.key === 'ArrowRight' ? clampRatio(current + 0.05) :
      event.key === 'Home' ? MIN_RATIO :
      event.key === 'End' ? MAX_RATIO :
      null;
    if (next === null) return;
    event.preventDefault();
    setRatio(next);
    writeStoredRatio(next);
  }, [ratio]);

  const transcriptPanelProps = isDesktop
    ? { role: 'region' as const, 'aria-label': 'Transcript', tabIndex: -1 }
    : {};
  const summaryPanelProps = isDesktop
    ? { role: 'region' as const, 'aria-label': 'Summary', tabIndex: -1 }
    : {};

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => onTabChange(value as MeetingDetailsTab)}
      className="flex flex-1 min-h-0 min-w-0 flex-col overflow-hidden"
    >
      <div className="shrink-0 bg-white px-2 md:hidden">
        <TabsList className="relative h-auto w-full justify-start rounded-none border-b border-gray-200 bg-transparent p-0">
          {TABS.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger
                key={tab.value}
                value={tab.value}
                className="relative z-10 flex items-center gap-2 rounded-none border-0 bg-transparent px-6 py-4 text-gray-600 data-[state=active]:bg-transparent data-[state=active]:text-blue-600 data-[state=active]:shadow-none hover:text-gray-900"
              >
                <Icon className="h-4 w-4" />
                {tab.label}
              </TabsTrigger>
            );
          })}
        </TabsList>
      </div>
      <div
        ref={containerRef}
        className="flex flex-1 min-h-0 min-w-0 flex-col md:flex-row"
        style={{ '--transcript-pane-width': `${ratio * 100}%` } as CSSProperties}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <TabsContent
          value="transcript"
          forceMount
          className="mt-0 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden data-[state=inactive]:hidden md:w-[var(--transcript-pane-width)] md:flex-none md:data-[state=inactive]:flex"
          {...transcriptPanelProps}
        >
          {transcript}
        </TabsContent>
        <div
          role="separator"
          aria-orientation="vertical"
          aria-valuenow={Math.round(ratio * 100)}
          aria-valuemin={Math.round(MIN_RATIO * 100)}
          aria-valuemax={Math.round(MAX_RATIO * 100)}
          aria-label="Resize transcript and summary"
          tabIndex={0}
          className="group relative z-10 hidden w-2 flex-shrink-0 cursor-col-resize items-stretch justify-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-inset md:flex"
          onPointerDown={onPointerDown}
          onKeyDown={onSeparatorKeyDown}
        >
          <div className="h-full w-px bg-gray-200 transition-[width,background-color] duration-150 ease-out group-hover:w-1 group-hover:bg-blue-400 group-active:w-1 group-active:bg-blue-500" />
        </div>
        <TabsContent
          value="summary"
          forceMount
          className="mt-0 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden data-[state=inactive]:hidden md:data-[state=inactive]:flex"
          {...summaryPanelProps}
        >
          {summary}
        </TabsContent>
      </div>
    </Tabs>
  );
}
