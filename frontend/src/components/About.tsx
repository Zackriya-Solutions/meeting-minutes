import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { UpdateDialog } from './UpdateDialog';
import { updateService, UpdateInfo } from '@/services/updateService';
import { Button } from './ui/button';
import { Mark, Wordmark } from './Logo';
import { Loader2, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';

/** What the product actually guarantees, as facts rather than pitch. */
const FACTS = [
  ['Audio', 'Captured and mixed on this machine'],
  ['Transcription', 'transcribe.cpp, running locally'],
  ['Summaries', 'A local model by default; external providers are opt-in'],
  ['Storage', 'A local database and audio files you own'],
] as const;

export function About() {
  const [currentVersion, setCurrentVersion] = useState<string>('0.4.0');
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [showUpdateDialog, setShowUpdateDialog] = useState(false);

  useEffect(() => {
    getVersion().then(setCurrentVersion).catch(console.error);
  }, []);

  const handleCheckForUpdates = async () => {
    setIsChecking(true);
    try {
      const info = await updateService.checkForUpdates(true);
      setUpdateInfo(info);
      if (info.available) setShowUpdateDialog(true);
      else toast.success('You are on the latest version');
    } catch (error: any) {
      console.error('Failed to check for updates:', error);
      toast.error('Could not check for updates', {
        description: error?.message || 'Unknown error',
      });
    } finally {
      setIsChecking(false);
    }
  };

  return (
    <div className="scrollbar-slim max-h-[75vh] overflow-y-auto pr-1">
      <div className="flex items-center gap-2.5">
        <Mark className="h-7 w-7 text-brand" />
        <div className="flex min-w-0 items-baseline gap-2">
          <Wordmark className="text-xl" />
          <span className="readout text-2xs text-ink-faint">v{currentVersion}</span>
        </div>
      </div>

      <p className="mt-3 max-w-[52ch] text-md leading-relaxed text-ink-muted">
        Meeting notes and summaries that never leave your machine.
      </p>

      {/* A privacy claim is only worth something if it is specific. */}
      <dl className="mt-5 divide-y divide-line border-y border-line">
        {FACTS.map(([term, detail]) => (
          <div key={term} className="flex gap-4 py-2">
            <dt className="w-24 shrink-0 text-sm font-medium text-ink">{term}</dt>
            <dd className="text-sm leading-relaxed text-ink-muted">{detail}</dd>
          </div>
        ))}
      </dl>

      <div className="mt-5 flex flex-wrap items-center gap-2">
        <Button
          onClick={handleCheckForUpdates}
          disabled={isChecking}
          variant="outline"
          size="sm"
        >
          {isChecking ? (
            <Loader2 className="animate-spin" aria-hidden />
          ) : (
            <RefreshCw aria-hidden />
          )}
          {isChecking ? 'Checking…' : 'Check for updates'}
        </Button>
        {updateInfo?.available && (
          <span className="text-sm text-brand">v{updateInfo.version} available</span>
        )}
      </div>

      {/* Upstream attribution. Conversationaly is a fork of Meetily; the credit
          stays regardless of what the product is called. */}
      <p className="mt-4 text-xs leading-relaxed text-ink-faint">
        Built on{' '}
        <button
          onClick={() =>
            invoke('open_external_url', {
              url: 'https://github.com/Zackriya-Solutions/meeting-minutes',
            }).catch(console.error)
          }
          className="underline underline-offset-2 transition-colors duration-fast hover:text-ink-muted"
        >
          Meetily
        </button>{' '}
        by Zackriya Solutions.
      </p>

      <UpdateDialog
        open={showUpdateDialog}
        onOpenChange={setShowUpdateDialog}
        updateInfo={updateInfo}
      />
    </div>
  );
}
