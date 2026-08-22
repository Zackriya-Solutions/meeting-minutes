'use client';

import React from 'react';
import { X, Sparkles, Loader2, CheckCircle2, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useOpenSpecSetup } from '@/hooks/useOpenSpecSetup';

/**
 * First-run, non-blocking prompt offering to install the OpenSpec CLI
 * dependency chain (see frontend-src-tauri/src/openspec/setup.rs).
 *
 * Rendered once per app lifetime: it self-hides permanently once the user
 * installs or explicitly skips (decision persisted on the Rust side via
 * tauri-plugin-store), and does not block any other UI - it's a dismissible
 * corner banner, not a modal, matching the "no bloqueante" requirement.
 *
 * Mount this once near the app root (see frontend/src/app/layout.tsx),
 * alongside DownloadProgressToastProvider - same "self-contained provider
 * that renders its own UI" pattern.
 */
export function OpenSpecSetupBanner() {
  const { phase, logLines, percent, errorMessage, install, skip, t } = useOpenSpecSetup();

  if (phase === 'installed' || phase === 'skipped' || phase === 'checking') {
    return null;
  }

  const isInstalling = phase === 'installing';
  const hasFailed = phase === 'error';

  return (
    <div className="fixed bottom-4 right-4 z-50 w-full max-w-sm rounded-lg border border-gray-200 bg-white shadow-lg">
      <div className="flex items-start gap-3 p-4">
        <div
          className={`flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center ${
            hasFailed ? 'bg-red-100' : 'bg-purple-100'
          }`}
        >
          {isInstalling ? (
            <Loader2 className="w-4 h-4 text-purple-600 animate-spin" />
          ) : hasFailed ? (
            <AlertCircle className="w-4 h-4 text-red-600" />
          ) : (
            <Sparkles className="w-4 h-4 text-purple-600" />
          )}
        </div>

        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-gray-900">
            {hasFailed ? t('openspec.setup.failed') : t('openspec.setup.title')}
          </p>
          <p className="mt-1 text-xs text-gray-500">
            {isInstalling ? t('openspec.setup.installing') : t('openspec.setup.description')}
          </p>

          {isInstalling && (
            <div className="mt-2 w-full h-1.5 bg-gray-200 rounded-full overflow-hidden">
              <div
                className="h-full bg-purple-600 rounded-full transition-all duration-300"
                style={{ width: `${percent ?? 0}%` }}
              />
            </div>
          )}

          {(isInstalling || hasFailed) && logLines.length > 0 && (
            <div className="mt-2 max-h-28 overflow-y-auto rounded bg-gray-950 p-2 font-mono text-[11px] leading-tight text-gray-100">
              {logLines.slice(-30).map((line, index) => (
                <div key={index} className="whitespace-pre-wrap break-all">
                  {line}
                </div>
              ))}
            </div>
          )}

          {hasFailed && errorMessage && (
            <p className="mt-1 text-xs text-red-600 break-words">{errorMessage}</p>
          )}

          {!isInstalling && (
            <div className="mt-3 flex items-center gap-2">
              <Button size="sm" onClick={() => void install()} className="h-7 px-2 text-xs">
                {hasFailed ? (
                  <>
                    <CheckCircle2 className="mr-1 h-3 w-3" />
                    {t('openspec.setup.retry')}
                  </>
                ) : (
                  t('openspec.setup.install')
                )}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void skip()}
                className="h-7 px-2 text-xs text-gray-500"
              >
                {t('openspec.setup.skip')}
              </Button>
            </div>
          )}
        </div>

        {!isInstalling && (
          <button
            type="button"
            onClick={() => void skip()}
            aria-label={t('openspec.setup.skip')}
            className="flex-shrink-0 text-gray-400 hover:text-gray-600"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>
    </div>
  );
}
