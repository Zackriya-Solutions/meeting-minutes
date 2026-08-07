import * as React from "react"

import { cn } from "@/lib/utils"

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          // Input wells are --sunken with a --border-strong outline, and focus
          // is carried by the border going brand plus the app's one global
          // :focus-visible ring — not by a 1px ring that replaced it. No
          // shadow (it does not float) and no md: type step (fixed scale).
          "flex h-9 w-full rounded-md border border-line-strong bg-sunken px-3 py-1 text-base text-ink transition-colors",
          "placeholder:text-ink-faint file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-ink",
          "hover:border-line-strong focus:border-brand focus:bg-elevated",
          "disabled:cursor-not-allowed disabled:opacity-45",
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
