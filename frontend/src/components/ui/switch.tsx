"use client"

import * as React from "react"
import * as SwitchPrimitives from "@radix-ui/react-switch"

import { cn } from "@/lib/utils"

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      "peer inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border border-[var(--border-strong)] transition-colors focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[var(--gold-soft)] disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-[var(--gold)] data-[state=unchecked]:bg-[var(--bg-elevated)]",
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        "pointer-events-none block h-4 w-4 rounded-full bg-[var(--fg1)] ring-0 transition-transform data-[state=checked]:translate-x-[22px] data-[state=unchecked]:translate-x-[3px]"
      )}
    />
  </SwitchPrimitives.Root>
))
Switch.displayName = SwitchPrimitives.Root.displayName

export { Switch }
