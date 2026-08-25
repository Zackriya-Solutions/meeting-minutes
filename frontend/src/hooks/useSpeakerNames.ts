'use client';

import { useCallback, useEffect, useState } from 'react';

type SpeakerNames = Record<string, string>;

const STORAGE_PREFIX = 'meetily:speakerNames:';

function readNames(meetingId: string): SpeakerNames {
  if (typeof window === 'undefined') return {};

  try {
    const value = window.localStorage.getItem(`${STORAGE_PREFIX}${meetingId}`);
    if (!value) return {};
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as SpeakerNames)
      : {};
  } catch {
    return {};
  }
}

function writeNames(meetingId: string, names: SpeakerNames): void {
  if (typeof window === 'undefined') return;

  try {
    window.localStorage.setItem(`${STORAGE_PREFIX}${meetingId}`, JSON.stringify(names));
  } catch {
    return;
  }
}

export function useSpeakerNames(meetingId: string | null | undefined) {
  const [allNames, setAllNames] = useState<SpeakerNames>({});

  useEffect(() => {
    setAllNames(meetingId ? readNames(meetingId) : {});
  }, [meetingId]);

  const setName = useCallback((speakerId: string, friendlyName: string) => {
    if (!meetingId) return;

    setAllNames((current) => {
      const next = { ...current };
      const trimmedName = friendlyName.trim();
      if (trimmedName) next[speakerId] = trimmedName;
      else delete next[speakerId];
      writeNames(meetingId, next);
      return next;
    });
  }, [meetingId]);

  return { allNames, setName };
}
