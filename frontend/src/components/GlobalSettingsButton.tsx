'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useTheme } from 'next-themes';
import { Icon } from '@/components/memento/Icon';
import { Button } from '@/components/ui/button';
import { useT } from '@/lib/i18n';
import { IconMoon, IconSun } from '@/vendor/deslop/material-symbols-react';

export function GlobalSettingsButton() {
  const pathname = usePathname();
  const { setTheme } = useTheme();
  const t = useT();

  const toggleTheme = () => {
    const isDark = document.documentElement.classList.contains('dark');
    setTheme(isDark ? 'light' : 'dark');
  };

  return (
    <div className="fixed right-4 top-4 z-50 flex items-center gap-2">
      <Button
        type="button"
        variant="outline"
        size="icon"
        className="rounded-full"
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
          variant="outline"
          size="icon"
          className="rounded-full"
        >
          <Link href="/settings" aria-label={t('Settings')} title={t('Settings')}>
            <Icon name="settings" size={22} />
          </Link>
        </Button>
      ) : null}
    </div>
  );
}
