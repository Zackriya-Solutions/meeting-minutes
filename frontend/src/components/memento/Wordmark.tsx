import React from 'react';
import { cn } from '@/lib/utils';

/**
 * The handwritten Memento wordmark, as it appears in the top-left of the main window.
 *
 * Drawn as a mask rather than an `<img>` so the glyphs take the current brand colour in both
 * themes. `width` keeps the 10:3 aspect the artwork was drawn at.
 */
export function Wordmark({
  width = 115.2,
  className,
}: {
  width?: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn('block aspect-[10/3] bg-[var(--deslop-primary-40)]', className)}
      style={{
        width,
        WebkitMaskImage: "url('/memento-logo-handwritten.svg')",
        maskImage: "url('/memento-logo-handwritten.svg')",
        WebkitMaskPosition: 'center',
        maskPosition: 'center',
        WebkitMaskRepeat: 'no-repeat',
        maskRepeat: 'no-repeat',
        WebkitMaskSize: 'contain',
        maskSize: 'contain',
      }}
    />
  );
}
