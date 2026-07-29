import React from "react";
import Image from "next/image";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";
import { useT } from '@/lib/i18n';
import { Button } from './ui/button';

interface LogoProps {
    isCollapsed: boolean;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed }, ref) => {
  const t = useT();
  return (
    <Dialog aria-describedby={undefined}>
      {isCollapsed ? (
        <DialogTrigger asChild>
          <Button ref={ref} type="button" variant="ghost" size="icon" className="mb-2">
            <Image src="/memento-mark.svg" alt="Memento" width={32} height={32} />
          </Button>
        </DialogTrigger>
      ) : (
        <DialogTrigger asChild>
          <span className="memento-logo mb-4 flex cursor-pointer items-center gap-3 transition-opacity hover:opacity-80">
            <Image src="/memento-mark.svg" alt="" width={30} height={30} />
            <span>memento</span>
          </span>
        </DialogTrigger>
      )}
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>{t('About Memento')}</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Logo.displayName = "Logo";

export default Logo;
