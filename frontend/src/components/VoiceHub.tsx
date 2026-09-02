'use client'

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ArrowRight, AudioLines, CalendarDays, Check, Clipboard, Clock3, Radio } from 'lucide-react'
import { useRouter } from 'next/navigation'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider'
import { PulseTalkMark } from '@/components/PulseTalkMark'

type DictationPhase = 'idle' | 'listening' | 'transcribing' | 'cleaning' | 'delivering' | 'completed' | 'failed' | 'cancelled'

type DictationState = {
  phase: DictationPhase
  message?: string | null
}

type ShortcutStatus = {
  enabled: boolean
  shortcut?: string | null
  message?: string | null
}

type DictationHistoryItem = {
  id: string
  phase: string
  finalText?: string | null
  failureMessage?: string | null
  startedAt: string
}

const phaseCopy: Record<DictationPhase, string> = {
  idle: 'Hold to talk',
  listening: 'Listening',
  transcribing: 'Turning speech into text',
  cleaning: 'Polishing transcript',
  delivering: 'Pasting at your cursor',
  completed: 'Pasted',
  failed: 'Saved to history',
  cancelled: 'Ready again',
}

export function VoiceHub({ meetings }: { meetings: CurrentMeeting[] }) {
  const router = useRouter()
  const [history, setHistory] = useState<DictationHistoryItem[]>([])
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null)
  const [dictation, setDictation] = useState<DictationState>({ phase: 'idle' })
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const mounted = useRef(true)
  const phaseResetTimer = useRef<number | null>(null)

  useEffect(() => {
    mounted.current = true
    Promise.all([
      invoke<DictationHistoryItem[]>('dictation_list_history', { limit: 20 }),
      invoke<ShortcutStatus>('dictation_get_shortcut_status'),
    ])
      .then(([items, status]) => {
        if (!mounted.current) return
        setHistory(items)
        setShortcut(status)
      })
      .catch(error => console.error('Failed to load voice hub data:', error))

    const unlisten = listen<DictationState>('dictation-state', event => {
      if (!mounted.current) return
      setDictation(event.payload)
      if (event.payload.phase === 'completed' || event.payload.phase === 'failed') {
        if (phaseResetTimer.current !== null) window.clearTimeout(phaseResetTimer.current)
        phaseResetTimer.current = window.setTimeout(() => {
          if (mounted.current) setDictation({ phase: 'idle' })
        }, event.payload.phase === 'completed' ? 1400 : 3400)
        window.setTimeout(() => {
          invoke<DictationHistoryItem[]>('dictation_list_history', { limit: 20 })
            .then(items => mounted.current && setHistory(items))
            .catch(error => console.error('Failed to refresh voice hub history:', error))
        }, 250)
      }
    })

    return () => {
      mounted.current = false
      if (phaseResetTimer.current !== null) window.clearTimeout(phaseResetTimer.current)
      unlisten.then(dispose => dispose())
    }
  }, [])

  const today = useMemo(() => {
    const now = new Date()
    return history.filter(item => {
      const date = new Date(item.startedAt)
      return date.getFullYear() === now.getFullYear() && date.getMonth() === now.getMonth() && date.getDate() === now.getDate()
    })
  }, [history])

  const todayWords = useMemo(
    () => today.reduce((sum, item) => sum + (item.finalText?.trim().split(/\s+/).filter(Boolean).length ?? 0), 0),
    [today],
  )

  const copy = async (id: string) => {
    await invoke('dictation_copy_history', { id })
    setCopiedId(id)
    window.setTimeout(() => setCopiedId(null), 1200)
  }

  const active = dictation.phase !== 'idle' && dictation.phase !== 'cancelled'
  const visibleHistory = history.filter(item => item.finalText || item.failureMessage).slice(0, 4)

  return (
    <div className="h-full overflow-y-auto bg-[#f3f5f6] px-6 pb-32 pt-7 text-[#19212a] lg:px-9">
      <div className="mx-auto max-w-6xl">
        <header className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] font-medium uppercase tracking-[0.18em] text-[#74808c]">
              {new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(new Date())}
            </p>
            <h1 className="mt-1 text-[34px] font-semibold tracking-[-0.035em]">Your voice workspace</h1>
            <p className="mt-1 text-sm text-[#687582]">Dictate anywhere, recover every word, keep meetings close to the work.</p>
          </div>
          <button
            onClick={() => router.push('/settings')}
            className="rounded-xl border border-[#d8dee3] bg-white px-4 py-2 text-sm font-medium shadow-sm transition hover:border-[#bbc6cf] hover:bg-[#fafbfc]"
          >
            Dictation settings
          </button>
        </header>

        <div className="mt-7 grid gap-4 lg:grid-cols-[minmax(0,1.55fr)_minmax(250px,.8fr)]">
          <button
            onClick={() => router.push('/dictation-history')}
            className="group relative min-h-[210px] overflow-hidden rounded-[26px] bg-[#17242c] p-6 text-left text-white shadow-[0_18px_45px_rgba(27,39,47,.14)]"
          >
            <div className="absolute -right-16 -top-20 h-64 w-64 rounded-full border border-[#30444b]" />
            <div className="absolute -right-4 -top-8 h-40 w-40 rounded-full border border-[#30444b]" />
            <div className="relative flex h-full flex-col justify-between">
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 rounded-lg border border-[#4a5a62] bg-[#213139] px-3 py-1.5 font-mono text-xs text-[#d9e0e3]">
                  <Radio className={`h-3.5 w-3.5 ${active ? 'text-[#42d4bb]' : 'text-[#93a1a8]'}`} />
                  {shortcut?.enabled ? shortcut.shortcut : 'Shortcut unavailable'}
                </span>
                <PulseTalkMark className="h-11 w-11 text-[#ff3b1f] transition-transform group-hover:scale-105" />
              </div>
              <div>
                <h2 className="text-2xl font-semibold tracking-[-0.025em]">{phaseCopy[dictation.phase]}</h2>
                <p className="mt-1 max-w-lg text-sm text-[#adbac0]">
                  {dictation.phase === 'listening'
                    ? `Release ${shortcut?.shortcut ?? 'the shortcut'} to paste at the original cursor.`
                    : dictation.message ?? 'Hold the shortcut while speaking. PulseTalq transcribes locally, then pastes where you were typing.'}
                </p>
                <div className="mt-5 flex h-11 items-center gap-1.5" aria-hidden="true">
                  {[18, 35, 24, 44, 28, 38, 20, 32, 17].map((height, index) => (
                    <i
                      key={index}
                      className={`w-1.5 rounded-full bg-[#42d4bb] transition-all ${active ? 'animate-pulse' : 'opacity-65'}`}
                      style={{ height }}
                    />
                  ))}
                </div>
              </div>
            </div>
          </button>

          <section className="rounded-[26px] border border-[#dce2e6] bg-white p-6 shadow-[0_1px_2px_rgba(20,28,35,.03)]">
            <p className="font-mono text-[11px] font-medium uppercase tracking-[0.15em] text-[#7b8791]">Today</p>
            <div className="mt-4 text-[44px] font-semibold leading-none tracking-[-0.05em]">{todayWords.toLocaleString()}</div>
            <p className="mt-2 text-sm text-[#6c7883]">words captured locally</p>
            <div className="my-5 h-px bg-[#e8ecef]" />
            <div className="flex items-center justify-between">
              <div><b className="text-lg">{today.length}</b><p className="text-xs text-[#7d8892]">dictations</p></div>
              <div className="text-right"><b className="text-lg">{history.filter(item => item.phase === 'completed').length}</b><p className="text-xs text-[#7d8892]">recent deliveries</p></div>
            </div>
          </section>
        </div>

        <div className="mt-4 grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(280px,.8fr)]">
          <section className="overflow-hidden rounded-[22px] border border-[#dce2e6] bg-white">
            <div className="flex items-center justify-between border-b border-[#e8ecef] px-5 py-4">
              <div><h2 className="font-semibold">Recent dictations</h2><p className="text-xs text-[#7b8791]">Saved before delivery</p></div>
              <button onClick={() => router.push('/dictation-history')} className="flex items-center gap-1.5 text-sm font-medium text-[#4863d6] hover:text-[#304ab9]">View history <ArrowRight className="h-4 w-4" /></button>
            </div>
            {visibleHistory.length === 0 ? (
              <div className="px-6 py-12 text-center text-sm text-[#77838e]">Your first dictation will appear here.</div>
            ) : visibleHistory.map(item => (
              <article key={item.id} className="grid grid-cols-[72px_minmax(0,1fr)_38px] gap-3 border-b border-[#edf0f2] px-5 py-4 last:border-0">
                <time className="pt-0.5 text-xs text-[#89939c]">{new Date(item.startedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>
                <div className="min-w-0"><p className={`line-clamp-2 text-sm leading-5 ${item.finalText ? 'text-[#2a333c]' : 'text-amber-800'}`}>{item.finalText ?? item.failureMessage}</p><span className="mt-1.5 inline-flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-[#909aa3]"><i className={`h-1.5 w-1.5 rounded-full ${item.phase === 'completed' ? 'bg-[#3bc5a9]' : 'bg-amber-400'}`} />{item.phase}</span></div>
                <button disabled={!item.finalText} onClick={() => copy(item.id)} aria-label="Copy dictation" className="grid h-8 w-8 place-items-center rounded-lg text-[#7b8791] transition hover:bg-[#f0f3f9] hover:text-[#4863d6] disabled:opacity-25">{copiedId === item.id ? <Check className="h-4 w-4 text-[#279d86]" /> : <Clipboard className="h-4 w-4" />}</button>
              </article>
            ))}
          </section>

          <section className="rounded-[22px] border border-[#dce2e6] bg-white p-5">
            <div className="flex items-start justify-between"><div><h2 className="font-semibold">Recent meetings</h2><p className="text-xs text-[#7b8791]">Stored on this device</p></div><CalendarDays className="h-5 w-5 text-[#ff3b1f]" /></div>
            <div className="mt-4 space-y-2">
              {meetings.slice(0, 4).map(meeting => (
                <button key={meeting.id} onClick={() => router.push(`/meeting-details?id=${meeting.id}`)} className="flex w-full items-center gap-3 rounded-xl border border-transparent px-2 py-2.5 text-left transition hover:border-[#e1e6eb] hover:bg-[#f8fafb]">
                  <span className="grid h-9 w-9 place-items-center rounded-xl bg-[#eef1ff] text-[#4d66d7]"><Clock3 className="h-4 w-4" /></span><span className="min-w-0 flex-1 truncate text-sm font-medium">{meeting.title}</span><ArrowRight className="h-4 w-4 text-[#a0a9b1]" />
                </button>
              ))}
              {meetings.length === 0 && <p className="rounded-xl border border-dashed border-[#d9e0e5] px-4 py-8 text-center text-sm text-[#7b8791]">Recorded meetings will stay connected here.</p>}
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}
