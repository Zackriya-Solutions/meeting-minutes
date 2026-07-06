import React from "react";
import Image from "next/image";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";

interface LogoProps {
    isCollapsed: boolean;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed }, ref) => {
  return (
    <Dialog aria-describedby={undefined}>
      {isCollapsed ? (
        <DialogTrigger asChild>
          <button
            ref={ref}
            aria-label="About Meetily"
            className="mb-2 flex cursor-pointer items-center justify-start border-none bg-transparent p-0 transition-opacity hover:opacity-80"
          >
            <Image src="/logo-collapsed.png" alt="Logo" width={40} height={32} />
          </button>
        </DialogTrigger>
      ) : (
        <DialogTrigger asChild>
          <span className="mb-2 block cursor-pointer items-center rounded-full border border-info/30 bg-info/10 text-center text-lg font-semibold text-foreground transition-opacity hover:opacity-80">
            <span>Meetily</span>
          </span>
        </DialogTrigger>
      )}
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>About Meetily</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Logo.displayName = "Logo";

export default Logo;
