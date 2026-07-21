'use client';

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { FileText, Sparkles } from 'lucide-react';
import { motion } from 'framer-motion';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

const STORAGE_KEY = 'meetily.meetingDetails.transcriptPaneRatio';
const DEFAULT_RATIO = 0.3;
const MIN_RATIO = 0.2;
const MAX_RATIO = 0.5;

const TABS = [
  { value: 'transcript' as const, label: 'Transcript', icon: FileText },
  { value: 'summary' as const, label: 'Summary', icon: Sparkles },
];

function readStoredRatio(): number {
  if (typeof window === 'undefined') return DEFAULT_RATIO;
  const raw = localStorage.getItem(STORAGE_KEY);
  const n = raw == null ? NaN : Number(raw);
  if (!Number.isFinite(n) || n < MIN_RATIO || n > MAX_RATIO) return DEFAULT_RATIO;
  return n;
}

export type MeetingDetailsTab = 'transcript' | 'summary';

interface MeetingDetailsSplitViewProps {
  transcript: ReactNode;
  summary: ReactNode;
  activeTab: MeetingDetailsTab;
  onTabChange: (tab: MeetingDetailsTab) => void;
}

/**
 * Desktop: side-by-side panes with a light, hover-emphasized drag separator.
 * Small screens: Settings-style Transcript / Summary tabs (one panel mounted).
 */
export function MeetingDetailsSplitView({
  transcript,
  summary,
  activeTab,
  onTabChange,
}: MeetingDetailsSplitViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [ratio, setRatio] = useState(DEFAULT_RATIO);
  const [isDesktop, setIsDesktop] = useState(true);
  const [underlineStyle, setUnderlineStyle] = useState({ left: 0, width: 0 });
  const dragging = useRef(false);

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

  useLayoutEffect(() => {
    if (isDesktop) return;
    const activeIndex = TABS.findIndex((tab) => tab.value === activeTab);
    const activeTabElement = tabRefs.current[activeIndex];
    if (activeTabElement) {
      const { offsetLeft, offsetWidth } = activeTabElement;
      setUnderlineStyle({ left: offsetLeft, width: offsetWidth });
    }
  }, [activeTab, isDesktop]);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    dragging.current = true;
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragging.current || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const next = (e.clientX - rect.left) / rect.width;
    const clamped = Math.min(MAX_RATIO, Math.max(MIN_RATIO, next));
    setRatio(clamped);
  }, []);

  const onPointerUp = useCallback(() => {
    if (!dragging.current) return;
    dragging.current = false;
    setRatio((r) => {
      localStorage.setItem(STORAGE_KEY, String(r));
      return r;
    });
  }, []);

  if (isDesktop) {
    return (
      <div
        ref={containerRef}
        className="flex flex-1 min-w-0 overflow-hidden"
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div
          className="min-w-0 h-full flex flex-col overflow-hidden"
          style={{ width: `${ratio * 100}%`, flexShrink: 0 }}
        >
          {transcript}
        </div>
        <div
          role="separator"
          aria-orientation="vertical"
          aria-valuenow={Math.round(ratio * 100)}
          aria-valuemin={Math.round(MIN_RATIO * 100)}
          aria-valuemax={Math.round(MAX_RATIO * 100)}
          aria-label="Resize transcript and summary"
          tabIndex={0}
          className="group relative z-10 flex w-2 flex-shrink-0 cursor-col-resize items-stretch justify-center"
          onPointerDown={onPointerDown}
        >
          <div
            className="h-full w-px bg-gray-200 transition-[width,background-color] duration-150 ease-out group-hover:w-1 group-hover:bg-blue-400 group-active:w-1 group-active:bg-blue-500"
          />
        </div>
        <div className="flex-1 min-w-0 h-full flex flex-col overflow-hidden">
          {summary}
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-1 min-w-0 flex-col overflow-hidden">
      <Tabs
        value={activeTab}
        onValueChange={(v) => onTabChange(v as MeetingDetailsTab)}
        className="flex flex-1 min-w-0 flex-col overflow-hidden"
      >
        <div className="bg-white px-2 shrink-0">
          <TabsList className="bg-transparent relative rounded-none border-b border-gray-200 p-0 h-auto w-full justify-start">
            {TABS.map((tab, index) => {
              const Icon = tab.icon;
              return (
                <TabsTrigger
                  key={tab.value}
                  value={tab.value}
                  ref={(el) => {
                    tabRefs.current[index] = el;
                  }}
                  className="flex items-center gap-2 px-6 py-4 bg-transparent rounded-none border-0 data-[state=active]:bg-transparent data-[state=active]:text-blue-600 data-[state=active]:shadow-none text-gray-600 hover:text-gray-900 relative z-10"
                >
                  <Icon className="w-4 h-4" />
                  {tab.label}
                </TabsTrigger>
              );
            })}
            <motion.div
              className="absolute bottom-0 z-20 h-0.5 bg-blue-600"
              layoutId="meeting-details-underline"
              style={{ left: underlineStyle.left, width: underlineStyle.width }}
              transition={{ type: 'spring', stiffness: 400, damping: 40 }}
            />
          </TabsList>
        </div>
        <div className="flex-1 min-h-0 min-w-0 overflow-hidden">
          {activeTab === 'transcript' ? transcript : summary}
        </div>
      </Tabs>
    </div>
  );
}
