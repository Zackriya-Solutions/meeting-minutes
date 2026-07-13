import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "mm-press inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full text-sm font-semibold transition-colors focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]",
        destructive:
          "bg-[var(--danger)] text-[var(--fg1)] hover:opacity-90",
        outline:
          "border border-[var(--border-strong)] bg-transparent text-[var(--fg1)] hover:bg-[var(--state-hover-bg)]",
        secondary:
          "bg-[var(--bg-elevated)] text-[var(--fg1)] hover:bg-[var(--state-hover-bg)]",
        ghost: "text-[var(--fg2)] hover:bg-[var(--state-hover-bg)] hover:text-[var(--fg1)]",
        link: "text-[var(--gold)] underline-offset-4 hover:underline",
        green: "bg-[var(--success)] text-[var(--fg-inverse)]",
        blue: "bg-[var(--gold)] text-[var(--fg-inverse)] hover:bg-[var(--gold-active)]",
        red: "bg-[var(--danger)] text-[var(--fg1)]",
        gray: "border border-[var(--border-strong)] bg-transparent text-[var(--fg2)] hover:bg-[var(--state-hover-bg)]",
      },
      size: {
        default: "h-11 px-5 py-2",
        sm: "h-9 px-4 text-xs",
        lg: "h-12 px-8",
        icon: "h-10 w-10 rounded-[16px]",
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
