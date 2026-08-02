"use client"

import {
  type ReactNode,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
} from "react"
import { useRouter } from "next/navigation"
import {
  HOME_SCROLL_POSITION_KEY,
  HomeMeetingList,
} from "@/app/_components/HomeMeetingList"
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerIndent,
  DrawerIndentBackground,
  DrawerProvider,
  DrawerTitle,
} from "@/components/ui/drawer"
import { MeetingDrawerProvider } from "@/contexts/MeetingDrawerContext"
import { useRouteDrawerLifecycle } from "@/hooks/useRouteDrawerLifecycle"
import { useT } from "@/lib/i18n"

export function MeetingDrawerShell({ children }: { children: ReactNode }) {
  const router = useRouter()
  const t = useT()
  const backgroundRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const storedPosition = Number(
      window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY)
    )

    if (backgroundRef.current && Number.isFinite(storedPosition)) {
      backgroundRef.current.scrollTop = Math.max(0, storedPosition)
    }
  }, [])

  const navigateHome = useCallback(() => {
    // Let the background width commit before replacing the route so the home
    // screen does not paint once at the drawer width and once at full width.
    window.requestAnimationFrame(() => {
      router.replace("/", { scroll: false })
    })
  }, [router])

  const {
    open,
    requestClose,
    onOpenChange,
    onOpenChangeComplete,
  } = useRouteDrawerLifecycle({ onClosed: navigateHome })
  const contextValue = useMemo(() => ({ close: requestClose }), [requestClose])

  return (
    <MeetingDrawerProvider value={contextValue}>
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
            <DrawerTitle className="sr-only">{t("Meeting")}</DrawerTitle>
            <DrawerDescription className="sr-only">
              {t("Meeting details")}
            </DrawerDescription>
            <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
          </DrawerContent>
        </Drawer>
      </DrawerProvider>
    </MeetingDrawerProvider>
  )
}
