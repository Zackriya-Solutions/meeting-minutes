'use client';

// Local memory search (spec §6/§10): hybrid dense+sparse search over the
// file-based meet-log, served by the Rust `meeting_log_search` command (which
// proxies the Python sidecar). Self-contained floating widget so it doesn't
// entangle with the existing meetings-DB sidebar search.

import { useEffect, useRef, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Search, X, FileText } from 'lucide-react';
import { toast } from 'sonner';

interface MemoryResult {
  snippet: string;
  meeting_date: string;
  session_time: string;
  topics: string[];
  file_path: string;
  summary_path: string;
  score: number;
}

export default function MeetLogSearch() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<MemoryResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

  // Cmd/Ctrl+Shift+F opens the panel.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        setOpen((o) => !o);
      }
      if (e.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    if (open) setTimeout(() => inputRef.current?.focus(), 50);
  }, [open]);

  const runSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([]);
      setSearched(false);
      return;
    }
    setLoading(true);
    try {
      const res = await invoke<MemoryResult[]>('meeting_log_search', { query: q, limit: 12 });
      setResults(res);
    } catch (e) {
      console.error('meeting_log_search failed:', e);
      toast.error('Search failed', { description: String(e) });
      setResults([]);
    } finally {
      setLoading(false);
      setSearched(true);
    }
  }, []);

  const onChange = (v: string) => {
    setQuery(v);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => runSearch(v), 350);
  };

  const reveal = async (path: string) => {
    try {
      await invoke('meeting_log_reveal', { path });
    } catch (e) {
      toast.error('Could not open file', { description: String(e) });
    }
  };

  return (
    <>
      {/* Floating trigger */}
      {!open && (
        <button
          onClick={() => setOpen(true)}
          title="Search meeting memory (⌘⇧F)"
          className="fixed bottom-5 right-5 z-40 flex items-center gap-2 rounded-full bg-gray-900 px-4 py-2.5 text-sm text-white shadow-lg hover:bg-gray-800"
        >
          <Search size={16} />
          Memory
        </button>
      )}

      {open && (
        <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-24" onClick={() => setOpen(false)}>
          <div
            className="w-[min(680px,92vw)] rounded-xl bg-white shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center gap-2 border-b border-gray-100 px-4 py-3">
              <Search size={18} className="text-gray-400" />
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => onChange(e.target.value)}
                placeholder="Search across all meetings (Thai + English)…"
                className="flex-1 bg-transparent text-base outline-none placeholder:text-gray-400"
              />
              {loading && <span className="text-xs text-gray-400">searching…</span>}
              <button onClick={() => setOpen(false)} className="text-gray-400 hover:text-gray-600">
                <X size={18} />
              </button>
            </div>

            <div className="max-h-[55vh] overflow-y-auto p-2">
              {results.map((r, i) => (
                <button
                  key={`${r.file_path}-${i}`}
                  onClick={() => reveal(r.summary_path || r.file_path)}
                  className="group flex w-full flex-col gap-1 rounded-lg px-3 py-2.5 text-left hover:bg-gray-50"
                >
                  <div className="flex items-center gap-2 text-xs text-gray-500">
                    <FileText size={13} />
                    <span>{r.meeting_date} · {r.session_time.replace(/-/g, ':')}</span>
                    {r.topics.length > 0 && (
                      <span className="ml-1 flex flex-wrap gap-1">
                        {r.topics.slice(0, 4).map((t) => (
                          <span key={t} className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] text-blue-600">
                            {t}
                          </span>
                        ))}
                      </span>
                    )}
                    <span className="ml-auto text-[10px] text-gray-300">{r.score.toFixed(3)}</span>
                  </div>
                  <p className="line-clamp-2 text-sm text-gray-700">{r.snippet}</p>
                </button>
              ))}

              {searched && !loading && results.length === 0 && (
                <p className="px-3 py-8 text-center text-sm text-gray-400">No matches found.</p>
              )}
              {!searched && (
                <p className="px-3 py-8 text-center text-sm text-gray-400">
                  Semantic + keyword search over your meeting transcripts &amp; summaries.
                </p>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
