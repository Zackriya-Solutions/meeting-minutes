"use client";

import { forwardRef, type InputHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { useShape } from "@/lib/shape-context";

/** Standalone form control adapted from Fluid Functionalism's InputField. */
const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  ({ className, type = "text", ...props }, ref) => {
    const shape = useShape();

    return (
      <input
        ref={ref}
        type={type}
        className={cn(
          `h-9 w-full ${shape.input} border border-[var(--primary-10)]`,
          "bg-transparent px-3 text-[13px] text-[var(--deslop-primary)] outline-none",
          "placeholder:text-[var(--deslop-primary-40)]",
          "transition-[background-color,border-color,box-shadow] duration-80",
          "hover:bg-[var(--primary-5)] focus:bg-[var(--elevation-2)]",
          "focus:ring-1 focus:ring-[var(--primary-10)] disabled:pointer-events-none disabled:opacity-50",
          className
        )}
        {...props}
      />
    );
  }
);

Input.displayName = "FluidInput";

export { Input };
