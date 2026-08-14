"use client"

import { useEffect } from "react"
import { LogicalSize } from "@tauri-apps/api/dpi"
import { isTauri } from "@tauri-apps/api/core"
import { getCurrentWindow } from "@tauri-apps/api/window"

export const MAIN_CONTENT_MIN_WIDTH = 480
export const ROUTE_DRAWER_MAIN_CONTENT_WIDTH = 600
export const ROUTE_DRAWER_LAYOUT_GAP = 24
const DRAWER_INSET = 12

export function getBaseWindowMinWidth(sidebarWidth: number) {
  return Math.ceil(sidebarWidth + MAIN_CONTENT_MIN_WIDTH)
}

export function getRouteDrawerWindowMinWidth(
  sidebarWidth: number,
  drawerWidth: number,
) {
  return Math.ceil(
    sidebarWidth
      + ROUTE_DRAWER_LAYOUT_GAP
      + ROUTE_DRAWER_MAIN_CONTENT_WIDTH
      + ROUTE_DRAWER_LAYOUT_GAP
      + drawerWidth
      + DRAWER_INSET
  )
}

/**
 * Keeps route-backed right drawers from squeezing the archive underneath the
 * left sidebar. The two explicit gaps keep the central column at least 24px
 * away from both the sidebar and the drawer at every supported window width.
 */
export function useRouteDrawerWindowConstraint(
  sidebarWidth: number,
  drawerWidth: number,
) {
  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false
    const minimumWidth = getRouteDrawerWindowMinWidth(sidebarWidth, drawerWidth)

    const applyWindowConstraint = async () => {
      const appWindow = getCurrentWindow()
      await appWindow.setMinSize(new LogicalSize(minimumWidth, 1))

      const [physicalSize, scaleFactor] = await Promise.all([
        appWindow.innerSize(),
        appWindow.scaleFactor(),
      ])
      const logicalSize = physicalSize.toLogical(scaleFactor)

      if (!cancelled && logicalSize.width < minimumWidth) {
        await appWindow.setSize(new LogicalSize(minimumWidth, logicalSize.height))
      }
    }

    applyWindowConstraint().catch((error) => {
      console.error("Failed to update route drawer window minimum width", error)
    })

    return () => {
      cancelled = true
      getCurrentWindow()
        .setMinSize(new LogicalSize(getBaseWindowMinWidth(sidebarWidth), 1))
        .catch((error) => {
          console.error("Failed to restore window minimum width", error)
        })
    }
  }, [drawerWidth, sidebarWidth])
}

export function useBaseWindowConstraint(
  sidebarWidth: number,
  enabled: boolean,
) {
  useEffect(() => {
    if (!isTauri() || !enabled) return

    let cancelled = false
    const minimumWidth = getBaseWindowMinWidth(sidebarWidth)

    const applyWindowConstraint = async () => {
      const appWindow = getCurrentWindow()
      await appWindow.setMinSize(new LogicalSize(minimumWidth, 1))

      const [physicalSize, scaleFactor] = await Promise.all([
        appWindow.innerSize(),
        appWindow.scaleFactor(),
      ])
      const logicalSize = physicalSize.toLogical(scaleFactor)

      if (!cancelled && logicalSize.width < minimumWidth) {
        await appWindow.setSize(new LogicalSize(minimumWidth, logicalSize.height))
      }
    }

    applyWindowConstraint().catch((error) => {
      console.error("Failed to update base window minimum width", error)
    })

    return () => {
      cancelled = true
    }
  }, [enabled, sidebarWidth])
}
