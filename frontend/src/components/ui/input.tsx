import * as React from "react"

import { cn } from "@/lib/utils"

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-12 w-full rounded-[16px] border border-[var(--border-subtle)] bg-[var(--surface-input)] px-4 py-2 text-base text-[var(--fg1)] transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-[var(--fg1)] placeholder:text-[var(--fg3)] focus-visible:border-[var(--gold)] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[var(--gold-soft)] disabled:cursor-not-allowed disabled:opacity-50 md:text-sm",
          className
        )}
        ref={ref}
        {...props}
      />
    )
  }
)
Input.displayName = "Input"

export { Input }
