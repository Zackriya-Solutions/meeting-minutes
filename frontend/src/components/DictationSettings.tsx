"use client"

import { invoke } from "@tauri-apps/api/core"
import { AudioLines, ClipboardCheck, FolderOpen, History, Keyboard, LockKeyhole, Mic2, PictureInPicture2, Sparkles } from "lucide-react"
import { useRouter } from "next/navigation"
import { type KeyboardEvent, useEffect, useState } from "react"
import { Switch } from '@/components/ui/switch'

type ShortcutStatus = {
  enabled: boolean
  shortcut?: string | null
  message?: string | null
}

export function DictationSettings() {
  const router = useRouter()
  const [status, setStatus] = useState<ShortcutStatus | null>(null)
  const [overlayEnabled, setOverlayEnabled] = useState<boolean | null>(null)
  const [savingOverlay, setSavingOverlay] = useState(false)
  const [openingDiagnostics, setOpeningDiagnostics] = useState(false)
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [capturingShortcut, setCapturingShortcut] = useState(false)
  const [savingShortcut, setSavingShortcut] = useState(false)
  const [shortcutError, setShortcutError] = useState<string | null>(null)

  useEffect(() => {
    invoke<ShortcutStatus>('dictation_get_shortcut_status')
      .then(setStatus)
      .catch(cause => setError(String(cause)))
    invoke<boolean>('dictation_get_overlay_enabled')
      .then(setOverlayEnabled)
      .catch(cause => setError(String(cause)))
  }, [])

  const captureShortcut = async (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!capturingShortcut) return
    event.preventDefault()
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return

    const key = event.code.startsWith('Key')
      ? event.code.slice(3)
      : event.code.startsWith('Digit')
        ? event.code.slice(5)
        : event.code === 'Space'
          ? 'Space'
          : event.code
    if (!key) return

    const modifiers = [
      event.ctrlKey ? 'Ctrl' : null,
      event.altKey ? 'Alt' : null,
      event.shiftKey ? 'Shift' : null,
      event.metaKey ? 'Cmd' : null,
    ].filter(Boolean)
    if (modifiers.length === 0) {
      setShortcutError('Hold Ctrl, Alt, Shift, or Cmd while choosing a key.')
      return
    }

    const nextShortcut = [...modifiers, key].join('+')
    setSavingShortcut(true)
    setShortcutError(null)
    try {
      const nextStatus = await invoke<ShortcutStatus>('dictation_set_shortcut', { shortcut: nextShortcut })
      setStatus(nextStatus)
      setCapturingShortcut(false)
    } catch (cause) {
      setShortcutError(String(cause))
    } finally {
      setSavingShortcut(false)
    }
  }

  const toggleOverlay = async (enabled: boolean) => {
    const previous = overlayEnabled
    setOverlayEnabled(enabled)
    setSavingOverlay(true)
    setError(null)
    try {
      await invoke('dictation_set_overlay_enabled', { enabled })
    } catch (cause) {
      setOverlayEnabled(previous)
      setError(`Could not update the floating overlay: ${String(cause)}`)
    } finally {
      setSavingOverlay(false)
    }
  }

  const openDiagnostics = async () => {
    setOpeningDiagnostics(true)
    setDiagnosticsError(null)
    try {
      await invoke('open_diagnostics_folder')
    } catch (cause) {
      setDiagnosticsError(`Could not open diagnostics: ${String(cause)}`)
    } finally {
      setOpeningDiagnostics(false)
    }
  }

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
              <div className="mt-4 flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  onClick={() => {
                    setShortcutError(null)
                    setCapturingShortcut(true)
                  }}
                  onKeyDown={captureShortcut}
                  onBlur={() => setCapturingShortcut(false)}
                  disabled={savingShortcut}
                  className={`inline-flex min-w-[180px] items-center justify-center gap-2 rounded-xl border px-3 py-2 text-sm font-semibold shadow-sm transition focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 ${capturingShortcut
                    ? 'border-indigo-500 bg-indigo-50 text-indigo-800'
                    : 'border-gray-300 bg-gray-50 text-gray-800 hover:border-indigo-300 hover:bg-indigo-50/50'
                    }`}
                  aria-label="Edit dictation shortcut"
                >
                  <Keyboard className="h-4 w-4" />
                  {capturingShortcut ? (savingShortcut ? 'Saving shortcut…' : 'Press a shortcut…') : status.shortcut}
                </button>
                {!capturingShortcut && <span className="text-xs text-gray-500">Click to change</span>}
              </div>
            ) : (
              <p className="mt-3 text-sm text-amber-700">{status?.message ?? 'Checking shortcut availability…'}</p>
            )}
            {(error || shortcutError) && <p className="mt-3 text-sm text-red-600">{error ?? shortcutError}</p>}
            <p className="mt-3 text-xs text-gray-500">Choose a modifier plus one key. The shortcut is saved for the next launch and takes effect immediately.</p>
          </div>
        </div>
      </section>

      <section className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-start gap-4">
          <div className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-indigo-50 text-indigo-600">
            <PictureInPicture2 className="h-5 w-5" />
          </div>
          <div className="min-w-0 flex-1">
            <label htmlFor="dictation-overlay-toggle" className="font-semibold text-gray-900">Floating dictation overlay</label>
            <p className="mt-1 text-sm leading-6 text-gray-600">Keep a small microphone above other windows. Hover over it to reveal the active shortcut; it expands automatically while PulseTalk listens and pastes.</p>
          </div>
          <Switch
            id="dictation-overlay-toggle"
            checked={overlayEnabled ?? true}
            disabled={overlayEnabled === null || savingOverlay}
            onCheckedChange={toggleOverlay}
            aria-label="Show floating dictation overlay"
          />
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
        <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
          <FolderOpen className="h-5 w-5 text-indigo-600" />
          <h3 className="mt-3 font-semibold text-gray-900">Diagnostics</h3>
          <p className="mt-1 text-sm leading-6 text-gray-600">Open the privacy-filtered support logs. PulseTalk keeps one active 1 MB file and four rotated archives.</p>
          <button
            onClick={openDiagnostics}
            disabled={openingDiagnostics}
            aria-busy={openingDiagnostics}
            className="mt-4 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm font-medium text-gray-800 transition hover:bg-gray-50 disabled:cursor-wait disabled:opacity-60"
          >
            {openingDiagnostics ? 'Opening…' : 'Open diagnostics folder'}
          </button>
          {diagnosticsError && <p className="mt-3 text-sm text-red-600" role="alert">{diagnosticsError}</p>}
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
