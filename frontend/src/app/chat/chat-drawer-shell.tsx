"use client"

import { type ReactNode, useCallback, useLayoutEffect, useRef } from "react"
import { useRouter } from "next/navigation"
import { HOME_SCROLL_POSITION_KEY, HomeMeetingList } from "@/app/_components/HomeMeetingList"
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerIndent,
  DrawerIndentBackground,
  DrawerProvider,
  DrawerTitle,
} from "@/components/ui/drawer"
import { useRouteDrawerLifecycle } from "@/hooks/useRouteDrawerLifecycle"
import { useT } from "@/lib/i18n"

export function ChatDrawerShell({ children }: { children: ReactNode }) {
  const router = useRouter()
  const t = useT()
  const backgroundRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const storedPosition = Number(window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY))

    if (backgroundRef.current && Number.isFinite(storedPosition)) {
      backgroundRef.current.scrollTop = Math.max(0, storedPosition)
    }
  }, [])

  const navigateHome = useCallback(() => {
    // Let the indent width settle before replacing the route. This prevents
    // the home layout from painting once at the pre-close width and once at
    // the full width, which reads as a jump when returning from the drawer.
    window.requestAnimationFrame(() => {
      router.replace("/", { scroll: false })
    })
  }, [router])

  const {
    open,
    onOpenChange,
    onOpenChangeComplete,
  } = useRouteDrawerLifecycle({ onClosed: navigateHome })

  return (
    <DrawerProvider>
      <DrawerIndentBackground
        className="route-drawer-background-surface"
        aria-hidden="true"
      />
      <DrawerIndent
        ref={backgroundRef}
        data-home-scroll-container
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
        <DrawerContent className="meeting-route-drawer" initialFocus={false}>
          <DrawerTitle className="sr-only">{t("Chat with archive")}</DrawerTitle>
          <DrawerDescription className="sr-only">
            {t("Ask your meeting archive")}
          </DrawerDescription>
          <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
        </DrawerContent>
      </Drawer>
    </DrawerProvider>
  )
}
