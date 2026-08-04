"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Search } from '@/components/deslop-icons';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { useLanguage } from '@/lib/i18n';
import type { PaginatedTranscriptsResponse, Transcript } from '@/types';

interface TranscriptSearchDialogProps {
  meetingId: string;
  transcripts: Transcript[];
  totalCount?: number;
  onSelect: (seconds: number) => void;
}

function normalizeSearchValue(value: string): string {
  return value
    .normalize('NFKC')
    .toLocaleLowerCase()
    .replaceAll('ё', 'е')
    .replace(/\s+/g, ' ')
    .trim();
}

function formatTimestamp(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainder = safeSeconds % 60;
  return `${minutes}:${String(remainder).padStart(2, '0')}`;
}

function buildExcerpt(text: string, query: string): string {
  const compact = text.replace(/\s+/g, ' ').trim();
  if (compact.length <= 132) return compact;

  const matchIndex = normalizeSearchValue(compact).indexOf(query);
  const start = Math.max(0, matchIndex - 42);
  const end = Math.min(compact.length, start + 132);
  return `${start > 0 ? '…' : ''}${compact.slice(start, end).trim()}${end < compact.length ? '…' : ''}`;
}

export function TranscriptSearchDialog({
  meetingId,
  transcripts,
  totalCount = transcripts.length,
  onSelect,
}: TranscriptSearchDialogProps) {
  const { t } = useLanguage();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [allTranscripts, setAllTranscripts] = useState(transcripts);
  const [isLoading, setIsLoading] = useState(false);
  const loadedMeetingRef = useRef<string | null>(null);
  const loadedCountRef = useRef(0);

  useEffect(() => {
    setAllTranscripts(transcripts);
    loadedMeetingRef.current = null;
    loadedCountRef.current = 0;
    setQuery('');
  }, [meetingId]);

  // Until a full set has been fetched, mirror the prop so segments that arrive
  // while a recording runs stay searchable.
  useEffect(() => {
    if (loadedMeetingRef.current === meetingId) return;
    setAllTranscripts(transcripts);
  }, [meetingId, transcripts]);

  const loadFullTranscript = useCallback(async () => {
    // The prop already carries every segment, so the effect above is enough. Not
    // marking the meeting as loaded is deliberate: it keeps that mirroring alive.
    if (totalCount <= transcripts.length) return;

    // A paginated meeting: fetch once, and again only if it has grown since.
    if (loadedMeetingRef.current === meetingId && loadedCountRef.current >= totalCount) return;

    setIsLoading(true);
    try {
      const response = await invoke<PaginatedTranscriptsResponse>('api_get_meeting_transcripts', {
        meetingId,
        limit: totalCount,
        offset: 0,
      });
      setAllTranscripts(response.transcripts);
      loadedMeetingRef.current = meetingId;
      loadedCountRef.current = totalCount;
    } catch (error) {
      console.warn('Failed to load the full transcript for local search:', error);
      loadedMeetingRef.current = null;
      setAllTranscripts(transcripts);
    } finally {
      setIsLoading(false);
    }
  }, [meetingId, totalCount, transcripts]);

  useEffect(() => {
    if (!open) return;
    void loadFullTranscript();
  }, [loadFullTranscript, open]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLocaleLowerCase() !== 'f') return;
      event.preventDefault();
      setOpen(true);
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  const normalizedQuery = normalizeSearchValue(query);
  const results = useMemo(() => {
    if (!normalizedQuery) return [];
    return allTranscripts
      .filter((transcript) => normalizeSearchValue(transcript.text).includes(normalizedQuery))
      .slice(0, 80);
  }, [allTranscripts, normalizedQuery]);

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen);
    if (!nextOpen) setQuery('');
  };

  return (
    <>
      <FluidButton
        type="button"
        variant="secondary"
        size="icon"
        active={open}
        aria-label={t('Find in transcript')}
        title={`${t('Find in transcript')} · ⌘F`}
        data-no-window-drag
        onClick={() => setOpen(true)}
        className="no-drag h-10 w-10 rounded-full shadow-none [&>span:first-child]:!bg-[var(--primary-5)] active:scale-[0.96]"
      >
        <Search size={18} strokeWidth={2} />
      </FluidButton>

      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent
          overlayClassName="!z-[2000]"
          className="!z-[2001] w-[min(560px,calc(100vw-32px))] gap-0 overflow-hidden border-[var(--primary-10)] bg-[var(--elevation-2)] p-0 shadow-xl"
        >
          <DialogTitle className="sr-only">{t('Find in transcript')}</DialogTitle>
          <Command shouldFilter={false} className="rounded-[inherit] bg-transparent">
            <CommandInput
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder={t('Find a word or phrase')}
              className="h-12 text-base"
            />
            <CommandList className="max-h-[min(440px,60vh)] p-1.5">
              {!normalizedQuery ? (
                <div className="px-3 py-10 text-center text-sm text-[var(--deslop-primary-60)]">
                  {t('Enter a word or phrase')}
                </div>
              ) : isLoading ? (
                <div className="px-3 py-10 text-center text-sm text-[var(--deslop-primary-60)]">
                  {t('Loading transcript…')}
                </div>
              ) : (
                <>
                  <CommandEmpty>{t('Nothing found')}</CommandEmpty>
                  {results.length > 0 && (
                    <CommandGroup heading={t('Matches')}>
                      {results.map((transcript) => {
                        const seconds = transcript.audio_start_time ?? transcript.chunk_start_time ?? 0;
                        return (
                          <CommandItem
                            key={transcript.id}
                            value={transcript.id}
                            onSelect={() => {
                              onSelect(seconds);
                              handleOpenChange(false);
                            }}
                            className="min-h-14 cursor-pointer items-start rounded-xl px-3 py-2.5 text-left data-[selected=true]:bg-[var(--primary-8)] data-[selected=true]:text-[var(--deslop-primary)] active:scale-[0.99]"
                          >
                            <span className="min-w-0 flex-1 text-sm leading-snug">
                              {buildExcerpt(transcript.text, normalizedQuery)}
                            </span>
                            <span className="mm-numeric shrink-0 pt-0.5 text-xs text-[var(--deslop-primary-60)]">
                              {formatTimestamp(seconds)}
                            </span>
                          </CommandItem>
                        );
                      })}
                    </CommandGroup>
                  )}
                </>
              )}
            </CommandList>
          </Command>
        </DialogContent>
      </Dialog>
    </>
  );
}
