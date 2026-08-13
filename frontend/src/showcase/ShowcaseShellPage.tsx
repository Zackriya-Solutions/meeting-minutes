'use client';

import { createPortal } from 'react-dom';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import CanonicalStorybookShell from '@/showcase/shell/CanonicalStorybookShell';
import { showcaseCatalog, showcaseItems } from '@/showcase/catalog';

export function ShowcaseShellPage() {
  const hostRef = useRef<HTMLDivElement>(null);
  const [shadowMount, setShadowMount] = useState<HTMLDivElement | null>(null);
  const [activeId, setActiveId] = useState<string>();
  const [theme, setTheme] = useState<'light' | 'dark'>('light');

  useEffect(() => {
    const initialId = window.location.hash.slice(1);
    if (showcaseItems.some((item) => item.id === initialId)) setActiveId(initialId);

    const host = hostRef.current;
    if (!host || host.shadowRoot) return;
    const shadow = host.attachShadow({ mode: 'open' });
    const stylesheet = document.createElement('link');
    stylesheet.rel = 'stylesheet';
    stylesheet.href = '/showcase/canonical-shell.css';
    const mount = document.createElement('div');
    shadow.append(stylesheet, mount);
    setShadowMount(mount);
  }, []);

  const select = useCallback((id: string) => {
    window.location.hash = id;
    setActiveId(id);
  }, []);

  const back = useCallback(() => {
    window.history.replaceState(null, '', window.location.pathname);
    setActiveId(undefined);
  }, []);

  const preview = useMemo(() => activeId ? (
    <iframe
      key={`${activeId}-${theme}`}
      title={`Preview: ${activeId}`}
      src={`/showcase-preview?scenario=${encodeURIComponent(activeId)}&theme=${theme}`}
      style={{ width: '100%', minHeight: 'calc(100vh - 64px)', border: 0, display: 'block' }}
    />
  ) : null, [activeId, theme]);

  return (
    <div ref={hostRef}>
      {shadowMount && createPortal(
        <CanonicalStorybookShell
          groups={showcaseCatalog.groups}
          activeId={activeId}
          onSelect={select}
          onBack={back}
          theme={theme}
          onToggleTheme={() => setTheme((value) => value === 'light' ? 'dark' : 'light')}
        >
          {preview}
        </CanonicalStorybookShell>,
        shadowMount,
      )}
    </div>
  );
}
