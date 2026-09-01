import React from "react";
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
          <button ref={ref} className="pulse-talq-mark pulse-talq-mark--compact" aria-label="About pulse talq">
            <span>p</span>
          </button>
        </DialogTrigger>
      ) : (
        <DialogTrigger asChild>
          <button ref={ref} className="pulse-talq-mark" aria-label="About pulse talq">
            <span>pulse </span><strong>talq</strong>
          </button>
        </DialogTrigger>
      )}
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>About pulse talq</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Logo.displayName = "Logo";

export default Logo;
