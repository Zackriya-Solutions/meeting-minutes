import * as React from "react"

import { cn } from "@/lib/utils"

const Textarea = React.forwardRef<
  HTMLTextAreaElement,
  React.ComponentProps<"textarea">
>(({ className, ...props }, ref) => {
  return (
    <textarea
      className={cn(
        // Same well treatment as Input — see input.tsx.
        "flex min-h-[60px] w-full rounded-md border border-line-strong bg-sunken px-3 py-2 text-base text-ink transition-colors",
        "placeholder:text-ink-faint",
        "hover:border-line-strong focus:border-brand focus:bg-elevated",
        "disabled:cursor-not-allowed disabled:opacity-45",
        className
      )}
      ref={ref}
      {...props}
    />
  )
})
Textarea.displayName = "Textarea"

export { Textarea }
