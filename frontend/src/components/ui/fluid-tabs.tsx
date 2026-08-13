"use client"

import * as React from "react"
import { AnimatePresence, motion } from "framer-motion"
import * as TabsPrimitive from "@radix-ui/react-tabs"

import { cn } from "@/lib/utils"
import { spring } from "@/lib/fluid/springs"

type ItemRect = {
  left: number
  top: number
  width: number
  height: number
}

type FluidTabsListContextValue = {
  registerItem: (index: number, element: HTMLButtonElement | null) => void
  setSelectedIndex: (index: number) => void
}

const FluidTabsListContext = React.createContext<FluidTabsListContextValue | null>(null)

const FluidTabs = TabsPrimitive.Root

const FluidTabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ children, className, ...props }, ref) => {
  const containerRef = React.useRef<HTMLDivElement | null>(null)
  const itemsRef = React.useRef(new Map<number, HTMLButtonElement>())
  const [itemRects, setItemRects] = React.useState<ItemRect[]>([])
  const [hoveredIndex, setHoveredIndex] = React.useState<number | null>(null)
  const [selectedIndex, setSelectedIndex] = React.useState<number | null>(null)

  const measureItems = React.useCallback(() => {
    const container = containerRef.current
    if (!container) return
    const containerRect = container.getBoundingClientRect()
    const nextRects: ItemRect[] = []
    itemsRef.current.forEach((element, index) => {
      const rect = element.getBoundingClientRect()
      nextRects[index] = {
        left: rect.left - containerRect.left,
        top: rect.top - containerRect.top,
        width: rect.width,
        height: rect.height,
      }
    })
    setItemRects(nextRects)
  }, [])

  const syncSelected = React.useCallback(() => {
    const container = containerRef.current
    if (!container) return
    const selected = container.querySelector<HTMLElement>('[data-fluid-tab-index][data-state="active"]')
    setSelectedIndex(selected ? Number(selected.getAttribute('data-fluid-tab-index')) : null)
  }, [])

  React.useLayoutEffect(() => {
    measureItems()
    syncSelected()
  }, [children, measureItems, syncSelected])

  React.useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const observer = new MutationObserver(() => {
      syncSelected()
      measureItems()
    })
    observer.observe(container, { subtree: true, attributes: true, attributeFilter: ['data-state'] })
    const resizeObserver = new ResizeObserver(measureItems)
    resizeObserver.observe(container)
    return () => {
      observer.disconnect()
      resizeObserver.disconnect()
    }
  }, [measureItems, syncSelected])

  const selectedRect = selectedIndex === null ? null : itemRects[selectedIndex]
  const hoverRect = hoveredIndex === null ? null : itemRects[hoveredIndex]
  const isHoveringSelected = hoveredIndex === selectedIndex

  const indexedChildren = React.Children.map(children, (child, index) => {
    if (!React.isValidElement(child) || typeof child.type === 'string') return child
    return React.cloneElement(child, { _fluidTabIndex: index } as Record<string, unknown>)
  })

  return (
    <FluidTabsListContext.Provider
      value={{
        registerItem: (index, element) => {
          if (element) itemsRef.current.set(index, element)
          else itemsRef.current.delete(index)
        },
        setSelectedIndex,
      }}
    >
      <TabsPrimitive.List
        ref={(node) => {
          containerRef.current = node
          if (typeof ref === 'function') ref(node)
          else if (ref) ref.current = node
        }}
        onMouseMove={(event) => {
          const rect = event.currentTarget.getBoundingClientRect()
          let closest: number | null = null
          let distance = Number.POSITIVE_INFINITY
          itemsRef.current.forEach((element, index) => {
            const itemRect = element.getBoundingClientRect()
            const nextDistance = Math.abs(event.clientX - (itemRect.left + itemRect.width / 2))
            if (nextDistance < distance) {
              distance = nextDistance
              closest = index
            }
          })
          setHoveredIndex(event.clientX >= rect.left && event.clientX <= rect.right ? closest : null)
        }}
        onMouseLeave={() => setHoveredIndex(null)}
        className={cn('relative inline-flex h-10 w-full items-center gap-0.5 rounded-xl bg-[var(--primary-5)] p-1', className)}
        {...props}
      >
        {selectedRect && (
          <motion.span
            aria-hidden="true"
            className="fluid-tabs-active-surface pointer-events-none absolute rounded-lg"
            initial={false}
            animate={{ ...selectedRect, opacity: isHoveringSelected ? 1 : 0.92 }}
            transition={spring.moderate}
          />
        )}
        <AnimatePresence>
          {hoverRect && !isHoveringSelected && (
            <motion.span
              aria-hidden="true"
              className="pointer-events-none absolute rounded-lg bg-[var(--primary-8)]"
              initial={{ ...hoverRect, opacity: 0 }}
              animate={{ ...hoverRect, opacity: 1 }}
              exit={{ opacity: 0, transition: spring.fast.exit }}
              transition={spring.fast}
            />
          )}
        </AnimatePresence>
        {indexedChildren}
      </TabsPrimitive.List>
    </FluidTabsListContext.Provider>
  )
})
FluidTabsList.displayName = 'FluidTabsList'

type FluidTabsTriggerProps = React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger> & {
  _fluidTabIndex?: number
}

const FluidTabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  FluidTabsTriggerProps
>(({ className, _fluidTabIndex = 0, onClick, onFocus, ...props }, ref) => {
  const context = React.useContext(FluidTabsListContext)
  const localRef = React.useRef<HTMLButtonElement | null>(null)

  React.useLayoutEffect(() => {
    context?.registerItem(_fluidTabIndex, localRef.current)
    return () => context?.registerItem(_fluidTabIndex, null)
  }, [_fluidTabIndex, context])

  return (
    <TabsPrimitive.Trigger
      ref={(node) => {
        localRef.current = node
        if (typeof ref === 'function') ref(node)
        else if (ref) ref.current = node
      }}
      data-fluid-tab-index={_fluidTabIndex}
      onClick={(event) => {
        context?.setSelectedIndex(_fluidTabIndex)
        onClick?.(event)
      }}
      onFocus={(event) => {
        context?.setSelectedIndex(_fluidTabIndex)
        onFocus?.(event)
      }}
      className={cn(
        'relative z-10 flex h-8 flex-1 items-center justify-center rounded-lg px-4 text-sm text-[var(--deslop-primary-60)] outline-none transition-[color,font-variation-settings] duration-80 data-[state=active]:text-[var(--deslop-primary)] focus-visible:ring-2 focus-visible:ring-[var(--primary-10)]',
        className,
      )}
      {...props}
    />
  )
})
FluidTabsTrigger.displayName = 'FluidTabsTrigger'

const FluidTabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Content ref={ref} className={cn('outline-none', className)} {...props} />
))
FluidTabsContent.displayName = 'FluidTabsContent'

export { FluidTabs, FluidTabsList, FluidTabsTrigger, FluidTabsContent }
