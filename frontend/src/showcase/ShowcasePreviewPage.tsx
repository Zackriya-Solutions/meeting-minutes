'use client';

import { useSearchParams } from 'next/navigation';
import { useEffect, useState } from 'react';
import { ThemeProvider } from 'next-themes';
import { LanguageProvider } from '@/lib/i18n';
import { TooltipProvider } from '@/components/ui/tooltip';
import { scenarioRegistry } from '@/showcase/scenario-registry';
import { installShowcaseTauriBoundary } from '@/showcase/tauri-boundary';

const TAURI_BOUNDARY_SCENARIOS = new Set([
  'update-check-provider',
  'shared-download-progress-toast',
]);

export function ShowcasePreviewPage() {
  const [mounted, setMounted] = useState(false);
  const searchParams = useSearchParams();
  const scenarioId = searchParams.get('scenario') ?? '';
  const theme = searchParams.get('theme') === 'dark' ? 'dark' : 'light';
  const Scenario = scenarioRegistry[scenarioId];

  useEffect(() => setMounted(true), []);

  if (!mounted) return null;
  if (!Scenario) return <main className="p-8 text-foreground">Сценарий не найден</main>;
  if (TAURI_BOUNDARY_SCENARIOS.has(scenarioId)) installShowcaseTauriBoundary();

  return (
    <ThemeProvider attribute="class" forcedTheme={theme} enableSystem={false}>
      <LanguageProvider>
        <TooltipProvider>
          <Scenario />
        </TooltipProvider>
      </LanguageProvider>
    </ThemeProvider>
  );
}
