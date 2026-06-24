'use client';

// Quick Note board (spec Feature 2): per-day checklist cards with checkbox,
// inline edit, delete, and a "carried from yesterday" badge. Persisted in
// SQLite via Tauri commands; end-of-day rollover runs at app launch.

import { useEffect, useRef, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { StickyNote, X, Trash2, CornerDownLeft } from 'lucide-react';
import { toast } from 'sonner';

interface QuickNoteItem {
  id: number;
  date: string;
  text: string;
  done: boolean;
  created_at: string;
  carried_from: string | null;
}

export default function QuickNote() {
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<QuickNoteItem[]>([]);
  const [draft, setDraft] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(async () => {
    try {
      const rows = await invoke<QuickNoteItem[]>('quick_notes_today');
      setItems(rows);
    } catch (e) {
      console.error('quick_notes_today failed:', e);
    }
  }, []);

  // Open via sidebar event or ⌘⇧N; refresh on open.
  useEffect(() => {
    const onOpen = () => setOpen(true);
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setOpen((o) => !o);
      }
      if (e.key === 'Escape') setOpen(false);
    };
    window.addEventListener('open-quick-note', onOpen);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('open-quick-note', onOpen);
      window.removeEventListener('keydown', onKey);
    };
  }, []);

  useEffect(() => {
    if (open) {
      refresh();
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open, refresh]);

  const add = async () => {
    const text = draft.trim();
    if (!text) return;
    setDraft('');
    try {
      await invoke('quick_note_add', { text });
      await refresh();
    } catch (e) {
      toast.error('Add failed', { description: String(e) });
    }
  };

  const toggle = async (it: QuickNoteItem) => {
    setItems((prev) => prev.map((x) => (x.id === it.id ? { ...x, done: !x.done } : x)));
    try {
      await invoke('quick_note_toggle', { id: it.id, done: !it.done });
    } catch (e) {
      toast.error('Update failed', { description: String(e) });
      refresh();
    }
  };

  const saveText = async (it: QuickNoteItem, text: string) => {
    const t = text.trim();
    if (t === it.text) return;
    try {
      await invoke('quick_note_update_text', { id: it.id, text: t });
      setItems((prev) => prev.map((x) => (x.id === it.id ? { ...x, text: t } : x)));
    } catch (e) {
      toast.error('Save failed', { description: String(e) });
    }
  };

  const remove = async (it: QuickNoteItem) => {
    setItems((prev) => prev.filter((x) => x.id !== it.id));
    try {
      await invoke('quick_note_delete', { id: it.id });
    } catch (e) {
      toast.error('Delete failed', { description: String(e) });
      refresh();
    }
  };

  const pending = items.filter((i) => !i.done).length;

  return (
    <>
      {!open && (
        <button
          onClick={() => setOpen(true)}
          title="Quick Note (⌘⇧N)"
          className="fixed bottom-5 right-36 z-40 flex items-center gap-2 rounded-full bg-amber-500 px-4 py-2.5 text-sm text-white shadow-lg hover:bg-amber-600"
        >
          <StickyNote size={16} />
          Quick Note{pending > 0 ? ` (${pending})` : ''}
        </button>
      )}

      {open && (
        <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-20" onClick={() => setOpen(false)}>
          <div className="flex max-h-[70vh] w-[min(560px,92vw)] flex-col rounded-xl bg-white shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
              <div className="flex items-center gap-2">
                <StickyNote size={18} className="text-amber-500" />
                <span className="font-medium text-gray-800">Quick Note — วันนี้</span>
              </div>
              <button onClick={() => setOpen(false)} className="text-gray-400 hover:text-gray-600">
                <X size={18} />
              </button>
            </div>

            <div className="flex items-center gap-2 px-4 py-2.5">
              <input
                ref={inputRef}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') add(); }}
                placeholder="เพิ่มการ์ดใหม่… (Enter)"
                className="flex-1 rounded-md border border-gray-200 px-3 py-2 text-sm outline-none focus:border-amber-400"
              />
              <button onClick={add} className="rounded-md bg-amber-500 px-3 py-2 text-sm text-white hover:bg-amber-600">
                เพิ่ม
              </button>
            </div>

            <div className="flex-1 overflow-y-auto px-2 pb-3">
              {items.length === 0 && (
                <p className="px-3 py-8 text-center text-sm text-gray-400">ยังไม่มีการ์ดวันนี้ — พิมพ์ด้านบนแล้วกด Enter</p>
              )}
              {items.map((it) => (
                <div key={it.id} className="group flex items-center gap-2 rounded-lg px-3 py-2 hover:bg-gray-50">
                  <button
                    onClick={() => toggle(it)}
                    title={it.done ? 'done ✅' : 'pending ❌'}
                    className={`flex h-5 w-5 flex-shrink-0 items-center justify-center rounded border text-xs ${
                      it.done ? 'border-green-500 bg-green-500 text-white' : 'border-gray-300 text-transparent hover:border-amber-400'
                    }`}
                  >
                    ✓
                  </button>
                  <input
                    defaultValue={it.text}
                    onBlur={(e) => saveText(it, e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
                    className={`flex-1 bg-transparent text-sm outline-none ${it.done ? 'text-gray-400 line-through' : 'text-gray-800'}`}
                  />
                  {it.carried_from && (
                    <span className="flex flex-shrink-0 items-center gap-0.5 rounded bg-amber-50 px-1.5 py-0.5 text-[10px] text-amber-600" title={`carried from ${it.carried_from}`}>
                      <CornerDownLeft size={10} /> เมื่อวาน
                    </span>
                  )}
                  <button
                    onClick={() => remove(it)}
                    title="ลบ"
                    className="opacity-0 group-hover:opacity-100 transition-opacity text-gray-300 hover:text-red-500 flex-shrink-0"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
