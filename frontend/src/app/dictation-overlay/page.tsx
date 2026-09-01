'use client'

import { listen } from '@tauri-apps/api/event'
import { useEffect, useMemo, useState } from 'react'

type DictationPhase =
  | 'idle'
  | 'listening'
  | 'transcribing'
  | 'cleaning'
  | 'delivering'
  | 'completed'
  | 'failed'
  | 'cancelled'

type DictationState = {
  phase: DictationPhase
  message?: string | null
}

const copy: Record<DictationPhase, string> = {
  idle: 'Ready',
  listening: 'Listening',
  transcribing: 'Turning speech into text',
  cleaning: 'Polishing',
  delivering: 'Pasting',
  completed: 'Pasted',
  failed: 'Dictation saved to history',
  cancelled: 'Cancelled',
}

export default function DictationOverlay() {
  const [state, setState] = useState<DictationState>({ phase: 'idle' })

  useEffect(() => {
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
    return () => {
      document.documentElement.style.background = ''
      document.body.style.background = ''
    }
  }, [])

  useEffect(() => {
    const unlisten = listen<DictationState>('dictation-state', event => {
      setState(event.payload)
    })
    return () => {
      unlisten.then(dispose => dispose())
    }
  }, [])

  const detail = useMemo(() => {
    if (state.phase === 'failed' && state.message) return state.message
    if (state.phase === 'listening') return 'Release Ctrl + Shift + Space to paste'
    return null
  }, [state])

  return (
    <main className={`dictation-floater dictation-${state.phase}`} aria-live="polite">
      <div className="dictation-signal" aria-hidden="true">
        <i />
        <i />
        <i />
        <i />
        <i />
      </div>
      <div className="dictation-copy">
        <div className="dictation-label">{copy[state.phase]}</div>
        {detail && <div className="dictation-detail">{detail}</div>}
      </div>
      <div className="dictation-mark" aria-hidden="true">P</div>
    </main>
  )
}
