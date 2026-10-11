import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Person } from '@/types';

/** People with remembered voices, for autocomplete and Settings → Speakers. */
export function usePeople(enabled: boolean) {
  const [people, setPeople] = useState<Person[]>([]);

  const refresh = useCallback(async () => {
    if (!enabled) {
      setPeople([]);
      return;
    }
    try {
      setPeople(await invoke<Person[]>('api_list_people'));
    } catch (error) {
      console.error('Failed to load people:', error);
    }
  }, [enabled]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { people, refresh };
}
