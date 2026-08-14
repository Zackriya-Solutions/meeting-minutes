"use client"

import { type ReactNode, useCallback, useLayoutEffect, useRef } from "react"
import { useRouter } from "next/navigation"
import { HOME_SCROLL_POSITION_KEY, HomeMeetingList } from "@/app/_components/HomeMeetingList"
import { useSidebar } from "@/components/Sidebar/SidebarProvider"
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerIndent,
  DrawerIndentBackground,
  DrawerProvider,
  DrawerTitle,
} from "@/components/ui/drawer"
import {
  useRouteDrawerLifecycle,
  waitForRouteDrawerBackgroundMotion,
} from "@/hooks/useRouteDrawerLifecycle"
import { useRouteDrawerWindowConstraint } from "@/hooks/useRouteDrawerWindowConstraint"
import { useT } from "@/lib/i18n"

const DRAWER_WIDTH = 450

export function RecordingDrawerShell({ children }: { children: ReactNode }) {
  const router = useRouter()
  const t = useT()
  const { sidebarWidth } = useSidebar()
  const backgroundRef = useRef<HTMLDivElement>(null)

  useRouteDrawerWindowConstraint(sidebarWidth, DRAWER_WIDTH)

  useLayoutEffect(() => {
    const storedPosition = Number(window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY))
    if (backgroundRef.current && Number.isFinite(storedPosition)) {
      backgroundRef.current.scrollTop = Math.max(0, storedPosition)
    }
  }, [])

  const navigateHome = useCallback(async () => {
    await waitForRouteDrawerBackgroundMotion(backgroundRef.current)
    window.requestAnimationFrame(() => {
      router.replace("/", { scroll: false })
    })
  }, [router])

  // Closable while recording: dismissing the drawer hands off to the home screen and
  // the recording keeps running, with the persistent sidebar button carrying Finish.
  const {
    open,
    onOpenChange,
    onOpenChangeComplete,
  } = useRouteDrawerLifecycle({
    onClosed: navigateHome,
  })

  return (
    <DrawerProvider>
      <DrawerIndentBackground
        className="route-drawer-background-surface"
        aria-hidden="true"
      />
      <DrawerIndent
        ref={backgroundRef}
        data-home-scroll-container
        data-route-drawer-open={open ? "" : undefined}
        className="route-drawer-background h-screen overflow-x-hidden overflow-y-auto"
      >
        <HomeMeetingList animateOnMount={false} />
      </DrawerIndent>
      <Drawer
        open={open}
        onOpenChange={onOpenChange}
        onOpenChangeComplete={onOpenChangeComplete}
        modal={false}
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
    </DrawerProvider>
  )
}
