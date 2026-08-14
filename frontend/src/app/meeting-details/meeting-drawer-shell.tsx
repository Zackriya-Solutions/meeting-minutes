"use client"

import {
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useRouter } from "next/navigation"
import {
  HOME_SCROLL_POSITION_KEY,
  HomeMeetingList,
} from "@/app/_components/HomeMeetingList"
import {
  useSidebar,
} from "@/components/Sidebar/SidebarProvider"
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
import {
  useRouteDrawerLifecycle,
  waitForRouteDrawerBackgroundMotion,
} from "@/hooks/useRouteDrawerLifecycle"
import {
  ROUTE_DRAWER_LAYOUT_GAP,
  ROUTE_DRAWER_MAIN_CONTENT_WIDTH,
  useRouteDrawerWindowConstraint,
} from "@/hooks/useRouteDrawerWindowConstraint"
import { useT } from "@/lib/i18n"

const DRAWER_WIDTH_STORAGE_KEY = "memento.meeting-drawer-width"
const DRAWER_DEFAULT_WIDTH = 450
const DRAWER_MAX_WIDTH = 700
const DRAWER_MIN_WIDTH = 450
const DRAWER_INSET = 12

function clampDrawerWidth(width: number, sidebarWidth: number) {
  const viewportMaximum = Math.max(
    DRAWER_MIN_WIDTH,
    window.innerWidth
      - sidebarWidth
      - ROUTE_DRAWER_MAIN_CONTENT_WIDTH
      - ROUTE_DRAWER_LAYOUT_GAP * 2
      - DRAWER_INSET
  )
  return Math.min(DRAWER_MAX_WIDTH, viewportMaximum, Math.max(DRAWER_MIN_WIDTH, width))
}

export function MeetingDrawerShell({ children }: { children: ReactNode }) {
  const router = useRouter()
  const t = useT()
  const { sidebarWidth } = useSidebar()
  const backgroundRef = useRef<HTMLDivElement>(null)
  const resizeStartRef = useRef<{ x: number; width: number } | null>(null)
  const widthRef = useRef(DRAWER_DEFAULT_WIDTH)
  const [drawerWidth, setDrawerWidth] = useState(DRAWER_DEFAULT_WIDTH)

  useRouteDrawerWindowConstraint(sidebarWidth, drawerWidth)

  const updateDrawerWidth = useCallback((nextWidth: number) => {
    const clamped = clampDrawerWidth(nextWidth, sidebarWidth)
    widthRef.current = clamped
    setDrawerWidth(clamped)
  }, [sidebarWidth])

  useLayoutEffect(() => {
    const storedPosition = Number(
      window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY)
    )

    if (backgroundRef.current && Number.isFinite(storedPosition)) {
      backgroundRef.current.scrollTop = Math.max(0, storedPosition)
    }

    const storedWidth = Number(window.localStorage.getItem(DRAWER_WIDTH_STORAGE_KEY))
    if (Number.isFinite(storedWidth) && storedWidth > 0) {
      updateDrawerWidth(storedWidth)
    }
  }, [updateDrawerWidth])

  useEffect(() => {
    const handleWindowResize = () => updateDrawerWidth(widthRef.current)
    window.addEventListener("resize", handleWindowResize)
    return () => {
      window.removeEventListener("resize", handleWindowResize)
      document.body.style.removeProperty("cursor")
      document.body.style.removeProperty("user-select")
    }
  }, [updateDrawerWidth])

  const navigateHome = useCallback(async () => {
    await waitForRouteDrawerBackgroundMotion(backgroundRef.current)
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

  const finishResize = useCallback((event: PointerEvent<HTMLDivElement>, allowClose: boolean) => {
    const start = resizeStartRef.current
    if (!start) return
    resizeStartRef.current = null
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    document.body.style.removeProperty("cursor")
    document.body.style.removeProperty("user-select")

    const draggedRight = event.clientX - start.x
    const closeThreshold = start.width - DRAWER_MIN_WIDTH + 64
    if (allowClose && draggedRight >= closeThreshold) {
      requestClose()
      return
    }

    window.localStorage.setItem(DRAWER_WIDTH_STORAGE_KEY, String(widthRef.current))
  }, [requestClose])

  const handleResizePointerDown = useCallback((event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    resizeStartRef.current = { x: event.clientX, width: widthRef.current }
    event.currentTarget.setPointerCapture(event.pointerId)
    document.body.style.cursor = "ew-resize"
    document.body.style.userSelect = "none"
  }, [])

  const handleResizePointerMove = useCallback((event: PointerEvent<HTMLDivElement>) => {
    const start = resizeStartRef.current
    if (!start) return
    updateDrawerWidth(start.width + start.x - event.clientX)
  }, [updateDrawerWidth])

  const handleResizeKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    updateDrawerWidth(widthRef.current + (event.key === "ArrowLeft" ? 20 : -20))
    window.localStorage.setItem(DRAWER_WIDTH_STORAGE_KEY, String(widthRef.current))
  }, [updateDrawerWidth])

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
          data-route-drawer-open={open ? "" : undefined}
          className="route-drawer-background h-screen overflow-x-hidden overflow-y-auto"
          style={{
            "--route-drawer-reserved-width": `${drawerWidth + DRAWER_INSET}px`,
          } as CSSProperties}
        >
          <HomeMeetingList animateOnMount={false} />
        </DrawerIndent>
        <Drawer
          open={open}
          onOpenChange={onOpenChange}
          onOpenChangeComplete={onOpenChangeComplete}
          modal={false}
          disablePointerDismissal
          swipeDirection="right"
        >
          <DrawerContent
            className="meeting-route-drawer"
            initialFocus={false}
            style={{ "--drawer-width": `${drawerWidth}px` } as CSSProperties}
          >
            <div
              role="separator"
              aria-label={t("Resize or close meeting panel")}
              aria-orientation="vertical"
              aria-valuemin={DRAWER_MIN_WIDTH}
              aria-valuemax={DRAWER_MAX_WIDTH}
              aria-valuenow={Math.round(drawerWidth)}
              tabIndex={0}
              className="meeting-drawer-resize-handle"
              onPointerDown={handleResizePointerDown}
              onPointerMove={handleResizePointerMove}
              onPointerUp={(event) => finishResize(event, true)}
              onPointerCancel={(event) => finishResize(event, false)}
              onKeyDown={handleResizeKeyDown}
            >
              <span
                aria-hidden="true"
                className="meeting-drawer-resize-handle-indicator"
              />
            </div>
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
