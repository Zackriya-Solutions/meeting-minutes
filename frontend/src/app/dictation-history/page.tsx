'use client'

import { invoke } from '@tauri-apps/api/core'
import { Check, Clipboard, Mic2, RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'

type DictationHistoryItem = {
  id: string
  phase: string
  finalText?: string | null
  failureCode?: string | null
  failureMessage?: string | null
  retryable: boolean
  startedAt: string
}

export default function DictationHistoryPage() {
  const [items, setItems] = useState<DictationHistoryItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [copiedId, setCopiedId] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setItems(await invoke<DictationHistoryItem[]>('dictation_list_history', { limit: 100 }))
    } catch (cause) {
      setError(String(cause))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const copy = async (id: string) => {
    try {
      await invoke('dictation_copy_history', { id })
      setCopiedId(id)
      window.setTimeout(() => setCopiedId(null), 1400)
    } catch (cause) {
      setError(String(cause))
    }
  }

  return (
    <div className="min-h-screen bg-[#f5f6f8] px-8 py-7 text-[#151923]">
      <header className="mx-auto flex max-w-4xl items-end justify-between border-b border-[#dfe2e8] pb-5">
        <div>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.16em] text-[#697386]">
            <Mic2 className="h-4 w-4 text-[#5577ff]" />
            PulseTalk
          </div>
          <h1 className="text-3xl font-semibold tracking-[-0.035em]">Dictation history</h1>
          <p className="mt-1.5 text-sm text-[#697386]">Every transcript is saved before PulseTalk tries to paste it.</p>
        </div>
        <button
          onClick={load}
          className="inline-flex items-center gap-2 rounded-xl border border-[#d7dbe3] bg-white px-3.5 py-2 text-sm font-medium shadow-sm transition hover:border-[#b9c3d7]"
        >
          <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          Refresh
        </button>
      </header>

      <main className="mx-auto mt-5 max-w-4xl space-y-2.5">
        {error && (
          <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900">{error}</div>
        )}
        {!loading && items.length === 0 && (
          <div className="rounded-2xl border border-dashed border-[#ccd1dc] bg-white px-6 py-14 text-center">
            <AudioEmpty />
            <p className="mt-4 font-medium">No dictations yet</p>
            <p className="mt-1 text-sm text-[#697386]">Hold Ctrl + Shift + Space anywhere, speak, then release.</p>
          </div>
        )}
        {items.map(item => (
          <article key={item.id} className="group grid grid-cols-[116px_minmax(0,1fr)_42px] gap-4 rounded-2xl border border-[#e0e3e9] bg-white px-4 py-3.5 shadow-[0_1px_2px_rgba(20,25,35,0.03)]">
            <div className="pt-0.5 text-xs text-[#7a8496]">
              <div>{new Date(item.startedAt).toLocaleDateString([], { month: 'short', day: 'numeric' })}</div>
              <div>{new Date(item.startedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</div>
            </div>
            <div className="min-w-0">
              {item.finalText ? (
                <p className="whitespace-pre-wrap text-[15px] leading-6 text-[#242a36]">{item.finalText}</p>
              ) : (
                <p className="text-sm font-medium text-amber-800">{item.failureMessage ?? 'Dictation did not finish.'}</p>
              )}
              <div className="mt-2 flex items-center gap-2 text-[11px] uppercase tracking-[0.1em] text-[#8a93a3]">
                <span className={`h-1.5 w-1.5 rounded-full ${item.phase === 'completed' ? 'bg-emerald-400' : 'bg-amber-400'}`} />
                {item.phase}
                {item.failureCode && <span>· {item.failureCode.replaceAll('_', ' ')}</span>}
              </div>
            </div>
            <button
              disabled={!item.finalText}
              onClick={() => copy(item.id)}
              aria-label="Copy dictation"
              className="grid h-9 w-9 place-items-center rounded-xl border border-transparent text-[#697386] transition hover:border-[#d9deea] hover:bg-[#f4f6fb] hover:text-[#405fdf] disabled:cursor-not-allowed disabled:opacity-25"
            >
              {copiedId === item.id ? <Check className="h-4 w-4 text-emerald-500" /> : <Clipboard className="h-4 w-4" />}
            </button>
          </article>
        ))}
      </main>
    </div>
  )
}

function AudioEmpty() {
  return (
    <div className="mx-auto flex h-12 w-12 items-center justify-center gap-1 rounded-2xl bg-[#eef1ff]">
      {[10, 20, 14, 24, 11].map((height, index) => (
        <i key={index} className="w-0.5 rounded-full bg-[#5577ff]" style={{ height }} />
      ))}
    </div>
  )
}
