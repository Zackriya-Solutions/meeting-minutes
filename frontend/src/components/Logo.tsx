import React from 'react';
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from './ui/dialog';
import { VisuallyHidden } from './ui/visually-hidden';
import { About } from './About';
import { cn } from '@/lib/utils';

/**
 * The mark is the pipeline: two capture streams (microphone, system audio)
 * converging into a single transcript. The junction dot is the capture point —
 * it turns red while recording, so the brand mark *is* the live indicator
 * rather than a decoration sitting next to one.
 */
export function Mark({
  live = false,
  className,
}: {
  live?: boolean;
  className?: string;
}) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className={cn('h-5 w-5', className)}
    >
      <g
        stroke="currentColor"
        strokeWidth={1.9}
        strokeLinecap="round"
        fill="none"
      >
        {/* Two capture streams in… */}
        <path d="M2.5 6.75C6 6.75 6.6 12 9.5 12" opacity={0.72} />
        <path d="M2.5 17.25C6 17.25 6.6 12 9.5 12" opacity={0.72} />
        {/* …one transcript out. */}
        <path d="M14.4 12H21.5" />
      </g>
      <circle
        cx={11.9}
        cy={12}
        r={2.3}
        className={cn(live ? 'fill-danger' : 'fill-current', live && 'animate-live')}
        style={{ transformOrigin: '11.9px 12px' }}
      />
    </svg>
  );
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        'font-semibold tracking-[-0.018em] text-ink whitespace-nowrap',
        className
      )}
    >
      Conversationaly
    </span>
  );
}

interface LogoProps {
  isCollapsed: boolean;
  live?: boolean;
}

/** Brand lockup in the rail. Opens About. */
const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(
  ({ isCollapsed, live = false }, ref) => (
    <Dialog aria-describedby={undefined}>
      <DialogTrigger asChild>
        <button
          ref={ref}
          title="About Conversationaly"
          className={cn(
            'group flex items-center rounded-md text-ink transition-colors duration-fast',
            'hover:bg-ink/5 active:bg-ink/10',
            isCollapsed ? 'h-9 w-9 justify-center' : 'h-9 w-full gap-2 px-2'
          )}
        >
          <Mark live={live} className="h-5 w-5 shrink-0 text-brand" />
          {!isCollapsed && <Wordmark className="text-base" />}
          <VisuallyHidden>About Conversationaly</VisuallyHidden>
        </button>
      </DialogTrigger>
      <DialogContent className="max-w-lg">
        <VisuallyHidden>
          <DialogTitle>About Conversationaly</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  )
);

Logo.displayName = 'Logo';

export default Logo;
