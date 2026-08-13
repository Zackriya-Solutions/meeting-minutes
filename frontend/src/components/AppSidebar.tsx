'use client';

import { usePathname, useRouter } from 'next/navigation';
import { useTheme } from 'next-themes';
import { AnimatePresence, motion } from 'framer-motion';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import {
  Sidebar,
  SidebarFooter,
} from '@/components/ui/sidebar';
import { useT } from '@/lib/i18n';
import { Wordmark } from '@/components/memento/Wordmark';
import { useSidebar as useMementoSidebar } from '@/components/Sidebar/SidebarProvider';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { fluidFontWeight, spring } from '@/lib/fluid/springs';
import { Button } from '@/components/ui/fluid-functional-button';
import { ShapeProvider } from '@/lib/shape-context';
import {
  IconCalendarToday,
  IconConstruction,
  IconMoon,
  IconSun,
  MaterialSymbol,
} from '@/vendor/deslop/primitives/material-symbols-react';

type FluidSidebarItem = {
  id: string;
  active?: boolean;
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick?: () => void;
};

type FluidItemRect = {
  top: number;
  left: number;
  width: number;
  height: number;
};

function FluidSidebarGroup({
  items,
  className = '',
}: {
  items: FluidSidebarItem[];
  className?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const animationFrameRef = useRef<number | null>(null);
  const sessionRef = useRef(0);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  const [itemRects, setItemRects] = useState<FluidItemRect[]>([]);

  const measureItems = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const nextRects = Array.from(
      container.querySelectorAll<HTMLElement>('[data-fluid-sidebar-index]')
    ).map((element) => ({
      top: element.offsetTop,
      left: element.offsetLeft,
      width: element.offsetWidth,
      height: element.offsetHeight,
    }));
    setItemRects(nextRects);
  }, []);

  useEffect(() => {
    measureItems();
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver(measureItems);
    observer.observe(container);
    return () => observer.disconnect();
  }, [items.length, measureItems]);

  useEffect(() => () => {
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
    }
  }, []);

  const handleMouseMove = (event: React.MouseEvent<HTMLDivElement>) => {
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
        if (items[index]?.disabled) return;
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
  const focusedRect = focusedIndex === null ? null : itemRects[focusedIndex];
  const selectedIndex = items.findIndex((item) => item.active);
  const selectedRect = selectedIndex < 0 ? null : itemRects[selectedIndex];

  return (
    <div
      ref={containerRef}
      role="group"
      className={`relative flex w-full flex-col ${className}`}
      onMouseEnter={() => {
        sessionRef.current += 1;
      }}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setActiveIndex(null)}
      onBlur={(event) => {
        if (!containerRef.current?.contains(event.relatedTarget as Node)) {
          setFocusedIndex(null);
          setActiveIndex(null);
        }
      }}
      onKeyDown={(event) => {
        if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        const buttons = Array.from(
          containerRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []
        );
        const currentIndex = buttons.indexOf(event.target as HTMLButtonElement);
        if (currentIndex < 0 || buttons.length === 0) return;
        event.preventDefault();
        const nextIndex = event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? buttons.length - 1
            : event.key === 'ArrowDown'
              ? (currentIndex + 1) % buttons.length
              : (currentIndex - 1 + buttons.length) % buttons.length;
        buttons[nextIndex]?.focus();
      }}
    >
      {selectedRect && (
        <motion.span
          aria-hidden="true"
          className="pointer-events-none absolute rounded-xl bg-[var(--deslop-primary-8)]"
          initial={false}
          animate={{ ...selectedRect, opacity: 1 }}
          transition={{ ...spring.moderate, opacity: { duration: 0.08 } }}
        />
      )}

      <AnimatePresence>
        {activeRect && (
          <motion.span
            key={sessionRef.current}
            aria-hidden="true"
            className="pointer-events-none absolute rounded-xl bg-[var(--deslop-primary-5)]"
            initial={{ ...activeRect, opacity: 0 }}
            animate={{ ...activeRect, opacity: 1 }}
            exit={{ opacity: 0, transition: spring.fast.exit }}
            transition={{ ...spring.fast, opacity: { duration: 0.08 } }}
          />
        )}
      </AnimatePresence>

      <AnimatePresence>
        {focusedRect && (
          <motion.span
            aria-hidden="true"
            className="pointer-events-none absolute z-20 rounded-[14px] border border-[hsl(var(--ring))]"
            initial={false}
            animate={{
              top: focusedRect.top - 2,
              left: focusedRect.left - 2,
              width: focusedRect.width + 4,
              height: focusedRect.height + 4,
            }}
            exit={{ opacity: 0, transition: spring.fast.exit }}
            transition={spring.fast}
          />
        )}
      </AnimatePresence>

      {items.map((item, index) => {
        const emphasized = item.active || activeIndex === index;
        return (
          <motion.button
            key={item.id}
            type="button"
            title={item.label}
            data-fluid-sidebar-index={index}
            data-active={item.active || undefined}
            disabled={item.disabled}
            onClick={item.onClick}
            onFocus={(event) => {
              setActiveIndex(index);
              setFocusedIndex(event.currentTarget.matches(':focus-visible') ? index : null);
            }}
            whileTap={item.disabled ? undefined : { scale: 0.985 }}
            transition={spring.fast}
            className="fluid-sidebar-action relative z-10 flex h-10 w-full items-center gap-3 rounded-xl px-3 text-left text-sm outline-none disabled:pointer-events-none disabled:opacity-45"
          >
            <span className="flex h-5 w-5 shrink-0 items-center justify-center">{item.icon}</span>
            <span className="inline-grid min-w-0">
              <span
                aria-hidden="true"
                className="invisible col-start-1 row-start-1 truncate"
                style={{ fontVariationSettings: fluidFontWeight.semibold }}
              >
                {item.label}
              </span>
              <span
                className="col-start-1 row-start-1 truncate transition-[font-variation-settings] duration-75"
                style={{
                  fontVariationSettings: emphasized
                    ? fluidFontWeight.semibold
                    : fluidFontWeight.normal,
                }}
              >
                {item.label}
              </span>
            </span>
          </motion.button>
        );
      })}
    </div>
  );
}

export function AppSidebar() {
  const pathname = usePathname();
  const router = useRouter();
  const { setTheme } = useTheme();
  const { openImportDialog } = useImportDialog();
  const { handleRecordingToggle } = useMementoSidebar();
  const t = useT();
  const meetingsActive = pathname === '/' || pathname.startsWith('/meeting-details');

  const toggleTheme = () => {
    const isDark = document.documentElement.classList.contains('dark');
    setTheme(isDark ? 'light' : 'dark');
  };

  return (
    <Sidebar side="left" variant="sidebar" collapsible="none" className="memento-settings-sidebar h-svh border-0 bg-transparent">
      <button
        type="button"
        className="no-drag flex cursor-pointer appearance-none border-0 bg-transparent px-5 pb-0 pt-[44px]"
        aria-label={t('Meetings')}
        onClick={() => router.push('/')}
      >
        <Wordmark />
      </button>
      <SidebarFooter className="border-0 px-3 pb-0 pt-6">
        <FluidSidebarGroup
          items={[
            {
              id: 'meetings',
              active: meetingsActive,
              icon: <IconCalendarToday size={20} weight={400} />,
              label: t('Meetings'),
              onClick: () => router.push('/'),
            },
            {
              id: 'upload-meeting',
              icon: <MaterialSymbol name="upload" size={20} weight={400} />,
              label: t('Upload meeting'),
              onClick: () => openImportDialog(),
            },
            {
              id: 'theme',
              icon: (
                <>
                  <span aria-hidden="true" className="inline-flex dark:hidden"><IconSun size={20} weight={400} /></span>
                  <span aria-hidden="true" className="hidden dark:inline-flex"><IconMoon size={20} weight={400} /></span>
                </>
              ),
              label: t('Switch theme'),
              onClick: toggleTheme,
            },
            {
              id: 'settings',
              active: pathname === '/settings',
              icon: <IconConstruction className="deslop-material-symbol--construction" size={20} weight={400} />,
              label: t('Settings'),
              onClick: () => router.push('/settings'),
            },
          ]}
        />
      </SidebarFooter>
      <div className="flex-1" />
      <SidebarFooter className="mt-auto border-0 px-6 pb-6 pt-6">
        <ShapeProvider defaultShape="pill" publishToRoot={false}>
          <Button
            type="button"
            variant="primary"
            className="h-11 w-full text-[var(--black)] [--background:var(--background-primary)] [--foreground:var(--accent-orange)]"
            onClick={handleRecordingToggle}
          >
            {t('Record meeting')}
          </Button>
        </ShapeProvider>
      </SidebarFooter>
    </Sidebar>
  );
}
