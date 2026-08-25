import { useCallback, useEffect, useState } from 'react';

// PR-32: Provider failover chain hook
// Persists user-configured provider chain in localStorage; default is a single primary provider
// (no failover) so existing users see no behaviour change.

export const PROVIDER_CHAIN_KEY = 'llm.provider_chain';
export const MAX_CHAIN_LENGTH = 5;

export interface ProviderChainEntry {
  provider: string;          // e.g. "openai", "ollama", "claude"
  model: string;             // e.g. "gpt-4o", "llama3.2:latest"
}

interface PersistedChain {
  enabled: boolean;
  entries: ProviderChainEntry[];
}

const DEFAULT_STATE: PersistedChain = { enabled: false, entries: [] };

function readFromStorage(): PersistedChain {
  if (typeof window === 'undefined') return DEFAULT_STATE;
  try {
    const raw = window.localStorage.getItem(PROVIDER_CHAIN_KEY);
    if (!raw) return DEFAULT_STATE;
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return DEFAULT_STATE;
    const enabled = parsed.enabled === true;
    const rawEntries = Array.isArray(parsed.entries) ? parsed.entries : [];
    const entries: ProviderChainEntry[] = [];
    for (const item of rawEntries) {
      if (!item || typeof item !== 'object') continue;
      const provider = typeof item.provider === 'string' ? item.provider : '';
      const model = typeof item.model === 'string' ? item.model : '';
      if (!provider || !model) continue;
      entries.push({ provider, model });
      if (entries.length >= MAX_CHAIN_LENGTH) break;
    }
    return { enabled, entries };
  } catch {
    return DEFAULT_STATE;
  }
}

function writeToStorage(state: PersistedChain): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(PROVIDER_CHAIN_KEY, JSON.stringify(state));
  } catch {
    // Quota / incognito 鈥?settings will reset at runtime; non-fatal.
  }
}
export function useProviderChain() {
  const [state, setState] = useState<PersistedChain>(() => readFromStorage());

  useEffect(() => {
    writeToStorage(state);
  }, [state]);

  const setEnabled = useCallback((enabled: boolean) => {
    setState(prev => ({ ...prev, enabled }));
  }, []);

  const setEntries = useCallback((entries: ProviderChainEntry[]) => {
    const trimmed = entries.slice(0, MAX_CHAIN_LENGTH);
    setState(prev => ({ ...prev, entries: trimmed }));
  }, []);

  const addEntry = useCallback((entry: ProviderChainEntry) => {
    setState(prev => {
      const filtered = prev.entries.filter(
        e => !(e.provider === entry.provider && e.model === entry.model)
      );
      const next = [...filtered, entry];
      return { ...prev, entries: next.slice(0, MAX_CHAIN_LENGTH) };
    });
  }, []);

  const removeEntry = useCallback((index: number) => {
    setState(prev => {
      const next = prev.entries.filter((_, i) => i !== index);
      return { ...prev, entries: next };
    });
  }, []);

  const moveEntry = useCallback((from: number, to: number) => {
    setState(prev => {
      if (from === to) return prev;
      if (from < 0 || from >= prev.entries.length) return prev;
      if (to < 0 || to >= prev.entries.length) return prev;
      const next = [...prev.entries];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return { ...prev, entries: next };
    });
  }, []);

  const reset = useCallback(() => {
    setState(DEFAULT_STATE);
  }, []);

  return {
    enabled: state.enabled,
    entries: state.entries,
    setEnabled,
    setEntries,
    addEntry,
    removeEntry,
    moveEntry,
    reset,
  };
}
export function providerChainToCommand(chain: { enabled: boolean; entries: { provider: string; model: string }[] }): Array<[string, string]> | null {
  if (!chain.enabled || chain.entries.length === 0) return null;
  return chain.entries.map(e => [e.provider, e.model] as [string, string]);
}
