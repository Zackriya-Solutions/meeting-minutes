"use client"

import { createContext, useContext } from "react"

type MeetingDrawerContextValue = {
  close: () => void
}

const MeetingDrawerContext = createContext<MeetingDrawerContextValue | null>(null)

export const MeetingDrawerProvider = MeetingDrawerContext.Provider

export function useMeetingDrawer() {
  return useContext(MeetingDrawerContext)
}
