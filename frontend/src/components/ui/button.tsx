import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

// One button vocabulary for the whole app — see /DESIGN.md. The ad-hoc
// `green` / `blue` / `red` / `gray` variants that used to live here were
// unreferenced and are gone; `destructive` is the only red affordance.
const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md",
    "text-sm font-medium",
    "transition-[background-color,border-color,color,opacity] duration-fast",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
    "disabled:pointer-events-none disabled:opacity-45",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ],
  {
    variants: {
      variant: {
        default:
          "bg-brand text-brand-ink hover:bg-brand-hover active:brightness-95",
        destructive:
          "bg-danger text-white hover:bg-danger-hover active:brightness-95",
        outline:
          "border border-line-strong bg-elevated text-ink hover:bg-ink/[0.04] active:bg-ink/[0.08]",
        secondary:
          "bg-sunken text-ink hover:bg-ink/[0.07] active:bg-ink/[0.11]",
        ghost:
          "text-ink-muted hover:bg-ink/[0.05] hover:text-ink active:bg-ink/[0.09]",
        link: "text-brand underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-3.5",
        sm: "h-8 px-2.5 text-sm",
        lg: "h-10 px-5 text-md",
        icon: "h-8 w-8",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button"
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = "Button"

export { Button, buttonVariants }
