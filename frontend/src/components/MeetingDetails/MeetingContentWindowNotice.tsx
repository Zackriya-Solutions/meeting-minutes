"use client";

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { useT } from '@/lib/i18n';

export interface MeetingContentWindowSuggestion {
  suggested: boolean;
  selected: boolean;
  primaryStartMs?: number | null;
  primaryEndMs?: number | null;
  excludedSegmentCount: number;
  gapMs?: number | null;
  excludedTextRatio?: number | null;
  confidence?: 'high' | 'medium' | string | null;
  reason?: string | null;
}

function timeLabel(milliseconds?: number | null): string {
  const seconds = Math.max(0, Math.floor((milliseconds ?? 0) / 1_000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export function MeetingContentWindowNotice({ meetingId }: { meetingId: string }) {
  const t = useT();
  const [suggestion, setSuggestion] = useState<MeetingContentWindowSuggestion | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const value = await invoke<MeetingContentWindowSuggestion>(
        'get_meeting_content_window_suggestion',
        { meetingId },
      );
      setSuggestion(value.suggested ? value : null);
    } catch (error) {
      console.error('Failed to inspect meeting content window:', error);
      setSuggestion(null);
    }
  }, [meetingId]);

  useEffect(() => {
    setSuggestion(null);
    void refresh();
  }, [refresh]);

  const choose = async (usePrimary: boolean) => {
    setBusy(true);
    try {
      const value = await invoke<MeetingContentWindowSuggestion>(
        'set_meeting_content_window_preference',
        { meetingId, usePrimary },
      );
      setSuggestion(value.suggested ? value : null);
      toast.success(usePrimary ? t('Primary meeting window selected') : t('Full transcript selected'));
    } catch (error) {
      console.error('Failed to save meeting content window:', error);
      toast.error(t('Failed to save meeting content window'));
    } finally {
      setBusy(false);
    }
  };

  if (!suggestion) return null;
  return (
    <div className="mx-4 mt-3 rounded-xl border border-[var(--gold-border)] bg-[var(--gold-soft)] px-3 py-2.5 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="font-medium text-[var(--fg1)]">{t('Possible trailing recording fragments')}</p>
          <p className="mt-0.5 text-xs text-[var(--fg3)]">
            {t('A long quiet gap separates the main meeting from a few later transcript fragments.')}{' '}
            {t('Suggested summary window')}: {timeLabel(suggestion.primaryStartMs)}–{timeLabel(suggestion.primaryEndMs)} ·{' '}
            {suggestion.excludedSegmentCount} {t('later fragments')}
          </p>
          <p className="mt-1 text-xs text-[var(--fg3)]">
            {t('The transcript and audio are never deleted. This choice only changes future summary input.')}
          </p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button
            size="sm"
            variant={suggestion.selected ? 'default' : 'outline'}
            disabled={busy}
            onClick={() => void choose(true)}
          >
            {t('Use primary window')}
          </Button>
          <Button
            size="sm"
            variant={!suggestion.selected ? 'default' : 'ghost'}
            disabled={busy}
            onClick={() => void choose(false)}
          >
            {t('Use full transcript')}
          </Button>
        </div>
      </div>
    </div>
  );
}
