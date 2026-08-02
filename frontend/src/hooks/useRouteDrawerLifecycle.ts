"use client"

import { useCallback, useEffect, useRef, useState } from "react"

type RouteDrawerPhase = "idle" | "opening" | "open" | "closing"

interface UseRouteDrawerLifecycleOptions {
  canClose?: boolean
  onClosed: () => void
}

const CLOSE_COMPLETION_FALLBACK_MS = 700

/**
 * Keeps a route-backed drawer and its route hand-off in one state machine.
 *
 * Base UI does not call `onOpenChange(true)` when a controlled drawer is
 * opened by setting `open={true}`. Tracking "has opened" in that callback
 * therefore leaves the route mounted after a visual close. The home screen is
 * visible at that point, but another push to the same drawer route is a no-op.
 *
 * This hook records intent before changing the controlled prop and always
 * finishes a requested close. The timeout is a watchdog for interrupted CSS
 * transitions/HMR; the normal path still waits for Base UI's completion event.
 */
export function useRouteDrawerLifecycle({
  canClose = true,
  onClosed,
}: UseRouteDrawerLifecycleOptions) {
  const [open, setOpen] = useState(false)
  const phaseRef = useRef<RouteDrawerPhase>("idle")
  const closeFallbackRef = useRef<number | null>(null)
  const onClosedRef = useRef(onClosed)

  onClosedRef.current = onClosed

  const clearCloseFallback = useCallback(() => {
    if (closeFallbackRef.current === null) return
    window.clearTimeout(closeFallbackRef.current)
    closeFallbackRef.current = null
  }, [])

  const finishClose = useCallback((source: "transition" | "fallback") => {
    if (phaseRef.current !== "closing") return

    clearCloseFallback()
    phaseRef.current = "idle"

    if (source === "fallback") {
      console.warn(
        "[RouteDrawer] Close transition did not complete; forcing route hand-off",
      )
    }

    onClosedRef.current()
  }, [clearCloseFallback])

  const requestClose = useCallback(() => {
    if (!canClose || phaseRef.current === "closing") return

    phaseRef.current = "closing"
    setOpen(false)
    clearCloseFallback()
    closeFallbackRef.current = window.setTimeout(() => {
      finishClose("fallback")
    }, CLOSE_COMPLETION_FALLBACK_MS)
  }, [canClose, clearCloseFallback, finishClose])

  const handleOpenChange = useCallback((nextOpen: boolean) => {
    if (!nextOpen) {
      requestClose()
      return
    }

    clearCloseFallback()
    phaseRef.current = "opening"
    setOpen(true)
  }, [clearCloseFallback, requestClose])

  const handleOpenChangeComplete = useCallback((nextOpen: boolean) => {
    if (nextOpen) {
      // An opening completion can arrive after a close was already requested.
      // Do not let that stale event overwrite the closing phase.
      if (phaseRef.current !== "closing") phaseRef.current = "open"
      return
    }

    finishClose("transition")
  }, [finishClose])

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      phaseRef.current = "opening"
      setOpen(true)
    })

    return () => {
      window.cancelAnimationFrame(frame)
      clearCloseFallback()
    }
  }, [clearCloseFallback])

  return {
    open,
    requestClose,
    onOpenChange: handleOpenChange,
    onOpenChangeComplete: handleOpenChangeComplete,
  }
}
