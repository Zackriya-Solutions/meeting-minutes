'use client';

// Settings selector for the meeting-log (file output) summary model. Lists
// installed Ollama models and lets the user override the env default
// (qwen3.5:9b → fallback qwen2.5:14b). Choice persisted in localStorage and
// re-applied to the Rust side on load.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

const LS_KEY = 'meetLogSummaryModel';

export default function MeetLogSummaryModel() {
  const [models, setModels] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [resolved, setResolved] = useState<string>('');

  useEffect(() => {
    (async () => {
      try {
        const list = await invoke<string[]>('meeting_log_list_models');
        setModels(list);
      } catch (e) {
        console.error('meeting_log_list_models failed:', e);
      }
      const saved = typeof window !== 'undefined' ? localStorage.getItem(LS_KEY) : null;
      if (saved) setSelected(saved);
      try {
        setResolved(await invoke<string>('meeting_log_get_summary_model'));
      } catch { /* sidecar/ollama may be down; ignore */ }
    })();
  }, []);

  const onChange = async (value: string) => {
    setSelected(value);
    const override = value === '' ? null : value;
    try {
      await invoke('meeting_log_set_summary_model', { model: override });
      if (override) localStorage.setItem(LS_KEY, override);
      else localStorage.removeItem(LS_KEY);
      setResolved(await invoke<string>('meeting_log_get_summary_model'));
      toast.success('Meeting summary model updated');
    } catch (e) {
      toast.error('Failed to set summary model', { description: String(e) });
    }
  };

  return (
    <div className="mt-4 rounded-lg border border-gray-200 p-4">
      <h3 className="text-sm font-semibold text-gray-800">Meeting-log summary model</h3>
      <p className="mt-1 text-xs text-gray-500">
        Model used for the per-session <code>summary.md</code>. Defaults to the
        env primary (<code>qwen3.5:9b</code>) and auto-falls back to
        <code> qwen2.5:14b</code> if it isn&apos;t installed.
      </p>
      <select
        value={selected}
        onChange={(e) => onChange(e.target.value)}
        className="mt-3 w-full rounded-md border border-gray-200 px-3 py-2 text-sm outline-none focus:border-blue-400"
      >
        <option value="">Use env default (with auto-fallback)</option>
        {models.map((m) => (
          <option key={m} value={m}>{m}</option>
        ))}
      </select>
      {resolved && (
        <p className="mt-2 text-xs text-gray-400">Currently resolving to: <span className="font-medium text-gray-600">{resolved}</span></p>
      )}
    </div>
  );
}
