'use client';

import React, { useCallback, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import { motion } from 'framer-motion';
import { ArrowLeft, Search, Loader2, SlidersHorizontal, X, Clock, FileText } from 'lucide-react';
import { cn } from '@/lib/utils';

// Mirrors the Rust `SearchHit` (search::hybrid).
interface SearchHit {
  chunk_id: number;
  meeting_id: string;
  meeting_title: string;
  start_ms: number;
  text: string;
  score: number;
  matched_terms: string[];
}

interface CollectionRef {
  id: number;
  name: string;
}

interface MeetingGroup {
  meeting_id: string;
  title: string;
  hits: SearchHit[];
}

function fmtTime(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, '0') : String(m);
  const base = `${mm}:${String(s).padStart(2, '0')}`;
  return h > 0 ? `${h}:${base}` : base;
}

export default function SearchPage() {
  const router = useRouter();

  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [showFilters, setShowFilters] = useState(false);
  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const [collections, setCollections] = useState<CollectionRef[]>([]);

  const loadedCollections = useRef(false);

  const openFilters = () => {
    setShowFilters((v) => !v);
    if (!loadedCollections.current) {
      loadedCollections.current = true;
      invoke<CollectionRef[]>('list_collections')
        .then((c) => setCollections(Array.isArray(c) ? c : []))
        .catch(() => setCollections([]));
    }
  };

  const activeFilterCount = (dateFrom ? 1 : 0) + (dateTo ? 1 : 0) + (collectionId != null ? 1 : 0);

  const runSearch = useCallback(
    async (text: string) => {
      const q = text.trim();
      if (!q || searching) return;
      setSearching(true);
      setError(null);
      setSearched(true);

      // Build filters. Upper date bound is made inclusive-of-day across RFC3339 / plain
      // date storage by extending to end-of-day.
      const filters: Record<string, unknown> = {};
      if (dateFrom) filters.date_from = dateFrom;
      if (dateTo) filters.date_to = `${dateTo}T23:59:59.999Z`;
      if (collectionId != null) filters.collection_ids = [collectionId];

      try {
        const hits = await invoke<SearchHit[]>('search_meetings', {
          query: q,
          filters: Object.keys(filters).length ? filters : null,
          limit: 50,
        });
        setResults(Array.isArray(hits) ? hits : []);
      } catch (e) {
        setError(typeof e === 'string' ? e : 'Поиск не удался.');
        setResults([]);
      } finally {
        setSearching(false);
      }
    },
    [searching, dateFrom, dateTo, collectionId],
  );

  // RRF order is preserved; first occurrence of a meeting = its best rank.
  const groups: MeetingGroup[] = useMemo(() => {
    const map = new Map<string, MeetingGroup>();
    for (const h of results) {
      let g = map.get(h.meeting_id);
      if (!g) {
        g = { meeting_id: h.meeting_id, title: h.meeting_title, hits: [] };
        map.set(h.meeting_id, g);
      }
      g.hits.push(h);
    }
    return Array.from(map.values());
  }, [results]);

  const openHit = (h: SearchHit) => {
    router.push(`/meeting-details?id=${encodeURIComponent(h.meeting_id)}&t=${Math.floor(h.start_ms / 1000)}`);
  };

  return (
    <div className="flex h-screen flex-col pr-8 pt-4">
      {/* Header + search bar */}
      <div className="border-b border-gray-100 pb-4">
        <div className="flex items-center gap-3">
          <button
            onClick={() => router.push('/')}
            className="rounded-lg p-2 text-gray-500 transition-colors hover:bg-gray-100"
            aria-label="Назад"
          >
            <ArrowLeft className="h-5 w-5" />
          </button>
          <h1 className="text-lg font-semibold text-gray-900">Поиск по встречам</h1>
        </div>

        <div className="mt-3 flex items-center gap-2">
          <div className="flex flex-1 items-center gap-2 rounded-xl border border-gray-200 px-3 focus-within:border-blue-400">
            <Search className="h-5 w-5 shrink-0 text-gray-400" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && runSearch(query)}
              placeholder="Найдите что угодно из ваших встреч…"
              className="h-11 flex-1 bg-transparent text-sm text-gray-800 outline-none"
            />
            {query && (
              <button onClick={() => setQuery('')} className="text-gray-400 hover:text-gray-600" aria-label="Очистить">
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
          <button
            onClick={openFilters}
            className={cn(
              'relative flex h-11 items-center gap-1.5 rounded-xl border px-3 text-sm transition-colors',
              showFilters || activeFilterCount
                ? 'border-blue-300 bg-blue-50 text-blue-700'
                : 'border-gray-200 text-gray-700 hover:bg-gray-100',
            )}
          >
            <SlidersHorizontal className="h-4 w-4" />
            Фильтры
            {activeFilterCount > 0 && (
              <span className="ml-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-blue-600 px-1 text-[10px] font-medium text-white">
                {activeFilterCount}
              </span>
            )}
          </button>
          <button
            onClick={() => runSearch(query)}
            disabled={searching || !query.trim()}
            className={cn(
              'flex h-11 items-center justify-center rounded-xl px-5 text-sm font-medium text-white transition-colors',
              searching || !query.trim() ? 'cursor-not-allowed bg-gray-300' : 'bg-blue-600 hover:bg-blue-700',
            )}
          >
            {searching ? <Loader2 className="h-5 w-5 animate-spin" /> : 'Найти'}
          </button>
        </div>

        {showFilters && (
          <div className="mt-3 flex flex-wrap items-end gap-4 rounded-xl bg-gray-50 p-3">
            <label className="flex flex-col gap-1 text-xs text-gray-500">
              С даты
              <input
                type="date"
                value={dateFrom}
                onChange={(e) => setDateFrom(e.target.value)}
                className="rounded-lg border border-gray-200 bg-white px-2 py-1.5 text-sm text-gray-700"
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-gray-500">
              По дату
              <input
                type="date"
                value={dateTo}
                onChange={(e) => setDateTo(e.target.value)}
                className="rounded-lg border border-gray-200 bg-white px-2 py-1.5 text-sm text-gray-700"
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-gray-500">
              Коллекция
              <select
                value={collectionId ?? ''}
                onChange={(e) => setCollectionId(e.target.value ? Number(e.target.value) : null)}
                className="rounded-lg border border-gray-200 bg-white px-2 py-1.5 text-sm text-gray-700"
              >
                <option value="">Любая</option>
                {collections.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            {activeFilterCount > 0 && (
              <button
                onClick={() => {
                  setDateFrom('');
                  setDateTo('');
                  setCollectionId(null);
                }}
                className="mb-1 text-xs text-gray-500 underline hover:text-gray-700"
              >
                Сбросить
              </button>
            )}
            <span className="mb-1 text-xs text-gray-400">
              Фильтр по спикеру появится после диаризации (Фаза&nbsp;2).
            </span>
          </div>
        )}
      </div>

      {/* Results */}
      <div className="flex-1 overflow-y-auto py-6">
        {!searched ? (
          <EmptyPrompt />
        ) : searching && results.length === 0 ? (
          <Centered>
            <Loader2 className="h-6 w-6 animate-spin text-gray-400" />
          </Centered>
        ) : error ? (
          <Centered>
            <p className="text-sm text-red-600">{error}</p>
          </Centered>
        ) : groups.length === 0 ? (
          <Centered>
            <p className="text-sm text-gray-500">Ничего не найдено. Попробуйте другие слова или ослабьте фильтры.</p>
          </Centered>
        ) : (
          <div className="mx-auto flex max-w-3xl flex-col gap-6">
            <p className="text-xs text-gray-400">
              {results.length} совпадени{plural(results.length)} в {groups.length} встреч{pluralMeet(groups.length)}
            </p>
            {groups.map((g) => (
              <div key={g.meeting_id}>
                <button
                  onClick={() => router.push(`/meeting-details?id=${encodeURIComponent(g.meeting_id)}`)}
                  className="mb-2 text-left text-sm font-semibold text-gray-900 hover:text-blue-700"
                >
                  {g.title || 'Без названия'}
                </button>
                <div className="flex flex-col gap-2">
                  {g.hits.map((h) => (
                    <motion.button
                      key={h.chunk_id}
                      initial={{ opacity: 0, y: 4 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.15 }}
                      onClick={() => openHit(h)}
                      className="group rounded-xl border border-gray-100 bg-white p-3 text-left transition-colors hover:border-blue-200 hover:bg-blue-50/40"
                    >
                      <div className="mb-1 flex items-center gap-1.5 text-xs text-gray-400">
                        <Clock className="h-3 w-3" />
                        {fmtTime(h.start_ms)}
                        <FileText className="ml-1 h-3 w-3 opacity-0 transition-opacity group-hover:opacity-100" />
                        <span className="opacity-0 transition-opacity group-hover:opacity-100">открыть</span>
                      </div>
                      <p className="line-clamp-3 text-sm leading-relaxed text-gray-700">
                        <Highlighted text={h.text} terms={h.matched_terms} />
                      </p>
                    </motion.button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Highlighted({ text, terms }: { text: string; terms: string[] }) {
  if (!terms || terms.length === 0) return <>{text}</>;
  const escaped = terms
    .filter(Boolean)
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  if (escaped.length === 0) return <>{text}</>;
  const re = new RegExp(`(${escaped.join('|')})`, 'gi');
  const termSet = new Set(terms.map((t) => t.toLowerCase()));
  const parts = text.split(re);
  return (
    <>
      {parts.map((p, i) =>
        termSet.has(p.toLowerCase()) ? (
          <mark key={i} className="rounded bg-yellow-200 px-0.5 text-gray-900">
            {p}
          </mark>
        ) : (
          <span key={i}>{p}</span>
        ),
      )}
    </>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return <div className="flex h-full items-center justify-center">{children}</div>;
}

function EmptyPrompt() {
  return (
    <div className="mx-auto flex max-w-md flex-col items-center pt-24 text-center">
      <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50">
        <Search className="h-7 w-7 text-blue-600" />
      </div>
      <h2 className="text-xl font-semibold text-gray-900">Поиск по всем встречам</h2>
      <p className="mt-2 text-sm text-gray-500">
        Гибридный поиск (по ключевым словам и по смыслу) находит нужный момент в любой записи.
        Нажмите на результат, чтобы открыть встречу на этом месте.
      </p>
    </div>
  );
}

function plural(n: number): string {
  const d = n % 10;
  const dd = n % 100;
  if (d === 1 && dd !== 11) return 'е';
  if (d >= 2 && d <= 4 && (dd < 10 || dd >= 20)) return 'я';
  return 'й';
}
function pluralMeet(n: number): string {
  const d = n % 10;
  const dd = n % 100;
  if (d === 1 && dd !== 11) return 'е';
  if (d >= 2 && d <= 4 && (dd < 10 || dd >= 20)) return 'ах';
  return 'ах';
}
