"use client"

import { invoke } from "@tauri-apps/api/core"
import { AudioLines, ClipboardCheck, History, LockKeyhole, Mic2, Sparkles } from "lucide-react"
import { useRouter } from "next/navigation"
import { useEffect, useState } from "react"

type ShortcutStatus = {
  enabled: boolean
  shortcut?: string | null
  message?: string | null
}

export function DictationSettings() {
  const router = useRouter()
  const [status, setStatus] = useState<ShortcutStatus | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    invoke<ShortcutStatus>('dictation_get_shortcut_status')
      .then(setStatus)
      .catch(cause => setError(String(cause)))
  }, [])

  return (
    <div className="space-y-6 pt-6">
      <section className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-start gap-4">
          <div className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-indigo-50 text-indigo-600">
            <AudioLines className="h-5 w-5" />
          </div>
          <div className="flex-1">
            <div className="flex items-center gap-2">
              <h2 className="text-lg font-semibold text-gray-900">Hold-to-talk activation</h2>
              <span className={`h-2 w-2 rounded-full ${status?.enabled ? 'bg-emerald-400' : 'bg-amber-400'}`} />
            </div>
            <p className="mt-1 text-sm text-gray-600">Hold while speaking and release when finished. PulseTalk records only while the shortcut is held.</p>
            {status?.enabled ? (
              <kbd className="mt-4 inline-flex rounded-lg border border-gray-300 bg-gray-50 px-3 py-1.5 text-sm font-semibold text-gray-800 shadow-sm">
                {status.shortcut}
              </kbd>
            ) : (
              <p className="mt-3 text-sm text-amber-700">{status?.message ?? 'Checking shortcut availability…'}</p>
            )}
            {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
            <p className="mt-3 text-xs text-gray-500">If the preferred shortcut is occupied, PulseTalk tries Ctrl+Alt+D and then Ctrl+Shift+F10. The active choice is always shown here and under General.</p>
          </div>
        </div>
      </section>

      <div className="grid gap-4 md:grid-cols-2">
        <SettingCard
          icon={Mic2}
          title="Local transcription"
          description="Uses the transcription provider and model selected in the Transcription tab. Audio stays on this machine when a local model is selected."
        />
        <SettingCard
          icon={Sparkles}
          title="Local cleanup"
          description="Repairs spacing and removes English hesitation fillers locally. If cleanup exceeds 150 ms or returns an error, PulseTalk pastes the exact raw transcript."
        />
        <SettingCard
          icon={ClipboardCheck}
          title="Paste at the original cursor"
          description="Replaces selected text or inserts at the caret, then restores all clipboard formats after the target has consumed the paste."
        />
        <SettingCard
          icon={LockKeyhole}
          title="Target protection"
          description="PulseTalk refuses to paste if focus moved, the original window closed, or the target runs at a higher Windows integrity level."
        />
        <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
          <History className="h-5 w-5 text-indigo-600" />
          <h3 className="mt-3 font-semibold text-gray-900">Recovery history</h3>
          <p className="mt-1 text-sm leading-6 text-gray-600">Every transcript is saved before paste, including failed deliveries.</p>
          <button
            onClick={() => router.push('/dictation-history')}
            className="mt-4 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm font-medium text-gray-800 transition hover:bg-gray-50"
          >
            Open dictation history
          </button>
        </section>
      </div>
    </div>
  )
}

function SettingCard({ icon: Icon, title, description }: { icon: typeof Mic2; title: string; description: string }) {
  return (
    <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
      <Icon className="h-5 w-5 text-indigo-600" />
      <h3 className="mt-3 font-semibold text-gray-900">{title}</h3>
      <p className="mt-1 text-sm leading-6 text-gray-600">{description}</p>
    </section>
  )
}
