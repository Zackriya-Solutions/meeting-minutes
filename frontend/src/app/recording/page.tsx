"use client"

import { useEffect, useMemo, useState } from "react"
import { Loader2 } from "@/components/deslop-icons"
import { VirtualizedTranscriptView } from "@/components/VirtualizedTranscriptView"
import { RecordOverlay } from "@/components/memento/RecordOverlay"
import { useSidebar } from "@/components/Sidebar/SidebarProvider"
import { useRecordingState, RecordingStatus } from "@/contexts/RecordingStateContext"
import { useTranscripts } from "@/contexts/TranscriptContext"
import { useRecordingStart } from "@/hooks/useRecordingStart"
import { useRecordingStateSync } from "@/hooks/useRecordingStateSync"
import { useRecordingStop } from "@/hooks/useRecordingStop"
import { useLanguage } from "@/lib/i18n"
import { getMeetingDisplayInfo } from "@/lib/meetingDisplay"
import { RecordingDrawerShell } from "./recording-drawer-shell"

const LOCKED_STATUSES = new Set<RecordingStatus>([
  RecordingStatus.STARTING,
  RecordingStatus.RECORDING,
  RecordingStatus.STOPPING,
  RecordingStatus.PROCESSING_TRANSCRIPTS,
  RecordingStatus.SAVING,
])

export default function RecordingPage() {
  const { lang, t } = useLanguage()
  const recordingState = useRecordingState()
  const { setIsMeetingActive } = useSidebar()
  const { transcripts, meetingTitle, currentMeetingId } = useTranscripts()
  const [isRecording, setIsRecording] = useState(recordingState.isRecording)

  const { setIsRecordingDisabled } = useRecordingStateSync(
    isRecording,
    setIsRecording,
    setIsMeetingActive,
  )
  useRecordingStart(isRecording, setIsRecording)
  const { handleRecordingStop, setIsStopping } = useRecordingStop(
    setIsRecording,
    setIsRecordingDisabled,
  )

  useEffect(() => {
    setIsRecording(recordingState.isRecording)
  }, [recordingState.isRecording])

  const segments = useMemo(() => transcripts.map((transcript) => ({
    id: transcript.id,
    timestamp: transcript.audio_start_time ?? 0,
    endTime: transcript.audio_end_time,
    text: transcript.text,
    confidence: transcript.confidence,
  })), [transcripts])

  const displayMeetingTitle = useMemo(() => {
    if (!meetingTitle || meetingTitle === "+ New Call") return t("New meeting")
    return getMeetingDisplayInfo({ title: meetingTitle }, lang).title
  }, [lang, meetingTitle, t])

  const locked = recordingState.isRecording || LOCKED_STATUSES.has(recordingState.status)
  const isStarting = recordingState.status === RecordingStatus.STARTING
  const isFinalizing =
    recordingState.status === RecordingStatus.STOPPING ||
    recordingState.status === RecordingStatus.PROCESSING_TRANSCRIPTS ||
    recordingState.status === RecordingStatus.SAVING

  const stopRecording = () => {
    setIsStopping(true)
    void handleRecordingStop(true)
  }

  return (
    <RecordingDrawerShell locked={locked}>
      <div className="flex h-full flex-col bg-[var(--elevation-1)]">
        <header className="shrink-0 border-b border-border px-[var(--drawer-content-inset)] py-4">
          <h1 className="memento-screen-title truncate text-foreground">
            {displayMeetingTitle}
          </h1>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {isFinalizing ? t("Saving meeting…") : isStarting ? t("Starting recording…") : t("Meeting recording")}
          </p>
        </header>

        <main className="min-h-0 flex-1 overflow-hidden">
          {isStarting && segments.length === 0 ? (
            <div className="flex h-full items-center justify-center text-muted-foreground">
              <Loader2 className="size-5 animate-spin" />
            </div>
          ) : (
            <VirtualizedTranscriptView
              segments={segments}
              isRecording={recordingState.isRecording}
              isPaused={recordingState.isPaused}
              isProcessing={recordingState.isProcessing}
              isStopping={recordingState.isStopping}
              enableStreaming={recordingState.isRecording}
              showConfidence
              viewportClassName="px-[var(--drawer-content-inset)]"
            />
          )}
        </main>

        {recordingState.isRecording && (
          <footer className="shrink-0 border-t border-border px-[var(--drawer-content-inset)] py-4">
            <RecordOverlay
              title={displayMeetingTitle}
              meetingId={currentMeetingId}
              onStop={stopRecording}
            />
          </footer>
        )}
        {isFinalizing && (
          <footer className="flex shrink-0 items-center justify-center gap-2 border-t border-border px-[var(--drawer-content-inset)] py-5 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            <span>{t("Saving meeting…")}</span>
          </footer>
        )}
      </div>
    </RecordingDrawerShell>
  )
}
