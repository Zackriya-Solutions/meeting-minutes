"use client"

import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { HOME_SCROLL_POSITION_KEY, HomeMeetingList } from "@/app/_components/HomeMeetingList"
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerTitle,
} from "@/components/ui/drawer"
import { useT } from "@/lib/i18n"

export function RecordingDrawerShell({
  children,
  locked,
}: {
  children: ReactNode
  locked: boolean
}) {
  const router = useRouter()
  const t = useT()
  const [open, setOpen] = useState(false)
  const backgroundRef = useRef<HTMLDivElement>(null)
  const hasRequestedOpenRef = useRef(false)
  const didNavigateRef = useRef(false)

  useLayoutEffect(() => {
    const storedPosition = Number(window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY))
    if (backgroundRef.current && Number.isFinite(storedPosition)) {
      backgroundRef.current.scrollTop = Math.max(0, storedPosition)
    }
  }, [])

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      hasRequestedOpenRef.current = true
      setOpen(true)
    })
    return () => window.cancelAnimationFrame(frame)
  }, [])

  const handleOpenChange = useCallback((nextOpen: boolean) => {
    if (!nextOpen && locked) return
    setOpen(nextOpen)
  }, [locked])

  const handleOpenChangeComplete = useCallback((nextOpen: boolean) => {
    if (nextOpen || !hasRequestedOpenRef.current || didNavigateRef.current) return
    didNavigateRef.current = true
    router.replace("/", { scroll: false })
  }, [router])

  return (
    <>
      <div
        ref={backgroundRef}
        data-home-scroll-container
        className={`route-drawer-background h-screen overflow-y-auto bg-background${open ? " is-open" : ""}`}
      >
        <HomeMeetingList animateOnMount={false} />
      </div>
      <Drawer
        open={open}
        onOpenChange={handleOpenChange}
        onOpenChangeComplete={handleOpenChangeComplete}
        swipeDirection="right"
        showSwipeHandle
      >
        <DrawerContent className="meeting-route-drawer recording-route-drawer" initialFocus={false}>
          <DrawerTitle className="sr-only">{t("Meeting recording")}</DrawerTitle>
          <DrawerDescription className="sr-only">
            {t("Live meeting transcript")}
          </DrawerDescription>
          <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
        </DrawerContent>
      </Drawer>
    </>
  )
}
