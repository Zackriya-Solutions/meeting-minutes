import type { ComponentProps } from "react";

import { cn } from "@/lib/utils";

/**
 * Standalone version of the loading indicator used by Fluid Functionalism's
 * Button. The path animation stays centered in a fixed SVG viewBox, so it does
 * not depend on the transform origin of an icon font or wrapper element.
 */
export function FluidSpinner({ className, ...props }: ComponentProps<"svg">) {
  return (
    <svg
      aria-hidden="true"
      className={cn("h-6 w-6", className)}
      viewBox="0 0 24 24"
      fill="none"
      {...props}
    >
      <path
        d="M 12 12 C 14 8.5 19 8.5 19 12 C 19 15.5 14 15.5 12 12 C 10 8.5 5 8.5 5 12 C 5 15.5 10 15.5 12 12 Z"
        stroke="currentColor"
        strokeWidth="1.125"
        strokeLinecap="round"
        pathLength="100"
        style={{
          strokeDasharray: "15 85",
          animation: "spinner-move 2s linear infinite, spinner-dash 4s ease-in-out infinite",
        }}
      />
    </svg>
  );
}
