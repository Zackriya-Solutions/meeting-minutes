import React from "react";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";
import { PulseTalkMark } from "./PulseTalkMark";

interface LogoProps {
    isCollapsed: boolean;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed }, ref) => {
  return (
    <Dialog aria-describedby={undefined}>
      {isCollapsed ? (
        <DialogTrigger asChild>
          <button ref={ref} className="flex items-center justify-start mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-80 transition-opacity">
            <PulseTalkMark className="h-9 w-9 text-[#526de8]" />
          </button>
        </DialogTrigger>
      ) : (
        <DialogTrigger asChild>
          <span className="mb-3 flex cursor-pointer items-center gap-2 rounded-xl border border-[#dfe4ef] bg-white px-3 py-2 text-left text-lg font-semibold text-[#1f2933] transition-opacity hover:opacity-80">
            <PulseTalkMark className="h-8 w-8 text-[#526de8]" />
            <span>PulseTalk</span>
          </span>
        </DialogTrigger>
      )}
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>About PulseTalk</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Logo.displayName = "Logo";

export default Logo;
