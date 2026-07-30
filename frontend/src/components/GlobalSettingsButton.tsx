'use client';

import Link from 'next/link';
import { usePathname, useRouter } from 'next/navigation';
import { useTheme } from 'next-themes';
import { Icon } from '@/components/memento/Icon';
import { Button } from '@/components/ui/button';
import { RecordingStatus, useRecordingState } from '@/contexts/RecordingStateContext';
import { useT } from '@/lib/i18n';
import { IconMoon, IconSun } from '@/vendor/deslop/material-symbols-react';

export function GlobalSettingsButton() {
  const pathname = usePathname();
  const router = useRouter();
  const { setTheme } = useTheme();
  const { isRecording, status } = useRecordingState();
  const t = useT();
  const canStartMeeting = !isRecording && (
    status === RecordingStatus.IDLE ||
    status === RecordingStatus.COMPLETED ||
    status === RecordingStatus.ERROR
  );

  const toggleTheme = () => {
    const isDark = document.documentElement.classList.contains('dark');
    setTheme(isDark ? 'light' : 'dark');
  };

  const startNewMeeting = () => {
    if (!canStartMeeting) return;

    sessionStorage.setItem('autoStartRecording', 'true');
    router.push('/recording');
  };

  return (
    <div className="global-header-actions fixed right-4 top-4 z-50 flex items-center gap-2">
      <Button
        type="button"
        variant="ghost"
        className="global-header-action mm-hover h-10 gap-2 rounded-full border-0 px-4 font-medium shadow-none"
        onClick={startNewMeeting}
        disabled={!canStartMeeting}
        aria-label={t('New meeting')}
        title={t('New meeting')}
      >
        <Icon name="plus" size={18} />
        {t('New meeting')}
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="global-header-action mm-hover h-10 w-10 rounded-full border-0 shadow-none"
        onClick={toggleTheme}
        aria-label={t('Switch theme')}
        title={t('Switch theme')}
      >
        <span aria-hidden="true" className="inline-flex dark:hidden">
          <IconMoon size={20} />
        </span>
        <span aria-hidden="true" className="hidden dark:inline-flex">
          <IconSun size={20} />
        </span>
      </Button>

      {pathname !== '/settings' ? (
        <Button
          asChild
          type="button"
          variant="ghost"
          size="icon"
          className="global-header-action mm-hover h-10 w-10 rounded-full border-0 shadow-none"
        >
          <Link href="/settings" aria-label={t('Settings')} title={t('Settings')}>
            <Icon name="settings" size={22} />
          </Link>
        </Button>
      ) : null}
    </div>
  );
}
