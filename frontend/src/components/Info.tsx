import React from 'react';
import { Info as InfoIcon } from 'lucide-react';
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from './ui/dialog';
import { VisuallyHidden } from './ui/visually-hidden';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';
import { About } from './About';

/**
 * `isCollapsed` no longer changes the shape — the rail footer uses the same
 * icon button in both states. The prop stays for call-site compatibility.
 */
const Info = React.forwardRef<HTMLButtonElement, { isCollapsed?: boolean }>(
  (_props, ref) => (
    <Dialog aria-describedby={undefined}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button
              ref={ref}
              aria-label="About Conversationaly"
              className="flex h-8 w-8 items-center justify-center rounded-md text-ink-muted transition-colors duration-fast hover:bg-ink/5 hover:text-ink active:bg-ink/10"
            >
              <InfoIcon className="h-4 w-4" />
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">About</TooltipContent>
      </Tooltip>
      <DialogContent className="max-w-lg">
        <VisuallyHidden>
          <DialogTitle>About Conversationaly</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  )
);

Info.displayName = 'Info';

export default Info;
